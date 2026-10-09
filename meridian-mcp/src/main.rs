use anyhow::{anyhow, Result};
use axum::{
    extract::{Json, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::post,
    Router,
};
use clap::Parser;
use futures::stream::{self, StreamExt};
use governor::{Quota, RateLimiter};
use pay_equity_engine::analysis::{
    calculate_efficient_frontier_inner, decompose_inner, optimize_inner, verify_inner,
};
use pay_equity_engine::defensibility::check_defensibility_inner;
use pay_equity_engine::types::{
    AllocationStrategy, DecompositionRequest, EfficientFrontierRequest, OptimizationRequest,
    OptimizationTarget, ProposedAdjustment, RangeTarget, VerificationRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::num::NonZeroU32;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use subtle::ConstantTimeEq;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on (if not set, runs in stdio mode)
    #[arg(short, long, env = "PORT")]
    port: Option<u16>,

    /// Transport mode: stdio (default) or sse
    #[arg(long, env = "MCP_TRANSPORT")]
    transport: Option<String>,

    /// API Key for HTTP authentication
    #[arg(long, env = "MCP_API_KEY")]
    api_key: Option<String>,

    /// Rate limit (requests per minute) for stdio mode. Default: 60.
    #[arg(long, default_value = "60")]
    rate_limit: u32,
}

#[derive(Deserialize, Debug, Clone)]
struct JsonRpcRequest {
    _jsonrpc: String,
    method: String,
    params: Option<Value>,
    id: Option<Value>,
}

#[derive(Serialize, Debug)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
}

// ... Parameter structs ...
#[derive(Deserialize)]
struct McpDecompositionParams {
    pub csv_content: String,
    pub outcome_variable: String,
    pub group_variable: String,
    pub reference_group: String,
    pub predictors: Vec<String>,
    pub categorical_predictors: Option<Vec<String>>,
    pub three_fold: Option<bool>,
    pub quantile: Option<f64>,
    pub reference_coefficients: Option<String>,
    pub bootstrap_reps: Option<usize>,
}

impl From<McpDecompositionParams> for DecompositionRequest {
    fn from(p: McpDecompositionParams) -> Self {
        Self {
            csv_data: p.csv_content.into_bytes(),
            outcome_variable: p.outcome_variable,
            group_variable: p.group_variable,
            reference_group: p.reference_group,
            predictors: p.predictors,
            categorical_predictors: p.categorical_predictors,
            three_fold: p.three_fold,
            quantile: p.quantile,
            reference_coefficients: p.reference_coefficients,
            bootstrap_reps: p.bootstrap_reps,
        }
    }
}

#[derive(Deserialize)]
struct McpOptimizationParams {
    pub csv_content: String,
    pub outcome_variable: String,
    pub group_variable: String,
    pub reference_group: String,
    pub predictors: Vec<String>,
    pub categorical_predictors: Option<Vec<String>>,
    pub budget: f64,
    pub target_gap: Option<f64>,
    pub target: Option<String>,
    pub strategy: Option<String>,
    pub min_gap_pct: Option<f64>,
    pub forensic_mode: Option<bool>,
    pub adjust_both_groups: Option<bool>,
    pub confidence_level: Option<f64>,
    pub range_target: Option<String>,
}

#[derive(Deserialize)]
struct McpProposedAdjustment {
    pub index: usize,
    pub value: f64,
    pub predictor_overrides: Option<HashMap<String, String>>,
    /// 0017-MERIDIAN P4 stable key. Optional and forwarded verbatim: an MCP caller that echoes
    /// back the `row_key` the engine emitted gets key-resolved identity; one that omits it gets
    /// the unchanged `index` path.
    #[serde(default)]
    pub row_key: Option<String>,
}

impl From<McpProposedAdjustment> for ProposedAdjustment {
    fn from(p: McpProposedAdjustment) -> Self {
        Self {
            index: p.index,
            row_key: p.row_key,
            value: p.value,
            predictor_overrides: p.predictor_overrides,
        }
    }
}

#[derive(Deserialize)]
struct McpVerificationParams {
    #[serde(flatten)]
    pub decomposition_params: McpDecompositionParams,
    pub adjustments: Vec<McpProposedAdjustment>,
    /// Level of the prediction interval `check_defensibility` scores against (0120 S7).
    #[serde(default)]
    pub confidence_level: Option<f64>,
}

impl From<McpVerificationParams> for VerificationRequest {
    fn from(p: McpVerificationParams) -> Self {
        Self {
            decomposition_params: p.decomposition_params.into(),
            adjustments: p.adjustments.into_iter().map(|a| a.into()).collect(),
            confidence_level: p.confidence_level,
        }
    }
}

#[derive(Deserialize)]
struct McpFrontierParams {
    #[serde(flatten)]
    pub decomposition_params: McpDecompositionParams,
    /// Level whose complement is the significance threshold of each frontier point (0120 S7).
    #[serde(default)]
    pub confidence_level: Option<f64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    // Determine mode
    let is_sse = args.transport.as_deref() == Some("sse") || args.port.is_some();

    if is_sse {
        let port = args.port.unwrap_or(8084);
        let api_key = args.api_key.ok_or_else(|| {
            anyhow!("MCP_API_KEY is required for HTTP/SSE mode! Server refuses to run without authentication.")
        })?;

        info!(
            "Starting Meridian MCP server in HTTP/SSE mode on port {}",
            port
        );
        run_sse_server(port, api_key).await?;
    } else {
        info!("Starting Meridian MCP server in Stdio mode");
        run_stdio_server(args.rate_limit).await?;
    }

    Ok(())
}

async fn run_stdio_server(rate_limit_per_min: u32) -> Result<()> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    // Configure Rate Limiter
    let quota = Quota::per_minute(
        NonZeroU32::new(rate_limit_per_min).unwrap_or(NonZeroU32::new(60).unwrap()),
    );
    let limiter = RateLimiter::direct(quota);

    while let Some(line) = reader.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        // Enforce Rate Limit
        if limiter.check().is_err() {
            limiter.until_ready().await;
        }

        let req: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse request: {}", e);
                let response_json = serde_json::to_string(&JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(json!({
                        "code": -32700,
                        "message": "Parse error"
                    })),
                    id: None,
                })?;
                println!("{}", response_json);
                std::io::stdout().flush()?;
                continue;
            }
        };

        if let Some(response) = handle_protocol(req).await {
            let response_json = serde_json::to_string(&response)?;
            println!("{}", response_json);
            std::io::stdout().flush()?;
        }
    }
    Ok(())
}

// --- SSE Mode ---

struct Session {
    _id: String,
    _created_at: Instant,
}

struct AppState {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    api_key: String,
    rate_limiter: Arc<governor::DefaultDirectRateLimiter>,
}

async fn run_sse_server(port: u16, api_key: String) -> Result<()> {
    let quota = Quota::per_minute(NonZeroU32::new(60).unwrap());
    let rate_limiter = Arc::new(RateLimiter::direct(quota));

    let state = AppState {
        sessions: Arc::new(RwLock::new(HashMap::new())),
        api_key,
        rate_limiter,
    };

    let cors = CorsLayer::new()
        .allow_origin(axum::http::HeaderValue::from_static("http://127.0.0.1"))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::header::HeaderName::from_static("x-api-key"),
            axum::http::header::HeaderName::from_static("mcp-session-id"),
        ])
        .expose_headers([axum::http::header::HeaderName::from_static(
            "mcp-session-id",
        )]);

    let app = Router::new()
        .route(
            "/sse",
            post(handle_sse_post)
                .get(handle_sse_get)
                .delete(handle_sse_delete),
        )
        .route("/messages", post(handle_sse_post))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .with_state(Arc::new(state));

    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle_sse_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
    Json(req): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    if state.rate_limiter.check().is_err() {
        return (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response();
    }

    let is_initialize = req.method == "initialize";
    let is_notification = req.id.is_none();

    let session_id = if is_initialize {
        let new_id = Uuid::new_v4().to_string();
        let mut sessions = match state.sessions.write() {
            Ok(s) => s,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error: session state corrupted",
                )
                    .into_response()
            }
        };
        sessions.insert(
            new_id.clone(),
            Session {
                _id: new_id.clone(),
                _created_at: Instant::now(),
            },
        );
        Some(new_id)
    } else {
        let header_id = headers
            .get("mcp-session-id")
            .and_then(|h| h.to_str().ok().map(String::from));
        let query_id = query
            .get("sessionId")
            .cloned()
            .or_else(|| query.get("session_id").cloned());

        if let Some(id) = header_id.or(query_id) {
            let sessions = match state.sessions.read() {
                Ok(s) => s,
                Err(_) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Internal server error: session state corrupted",
                    )
                        .into_response()
                }
            };
            if sessions.contains_key(&id) {
                Some(id)
            } else {
                None
            }
        } else {
            None
        }
    };

    if !is_initialize && session_id.is_none() {
        return (
            StatusCode::UNAUTHORIZED,
            "Missing or invalid Mcp-Session-Id header",
        )
            .into_response();
    }

    let auth_header = headers
        .get("x-api-key")
        .or_else(|| headers.get("authorization"))
        .and_then(|h| h.to_str().ok());

    let authorized = match auth_header {
        Some(h) => safe_compare(h, &state.api_key),
        None => false,
    };

    if !authorized {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }

    let response_opt = handle_protocol(req).await;

    if is_notification {
        return StatusCode::ACCEPTED.into_response();
    }

    if let Some(resp) = response_opt {
        let mut response = Json(resp).into_response();
        response
            .headers_mut()
            .insert("Content-Type", HeaderValue::from_static("application/json"));

        if let Some(sid) = session_id {
            let hv = match HeaderValue::from_str(&sid) {
                Ok(v) => v,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            };
            response.headers_mut().insert("Mcp-Session-Id", hv);
        }
        response
    } else {
        StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}

async fn handle_sse_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if state.rate_limiter.check().is_err() {
        return (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response();
    }

    let auth_header = headers
        .get("x-api-key")
        .or_else(|| headers.get("authorization"))
        .and_then(|h| h.to_str().ok());

    let authorized = match auth_header {
        Some(h) => safe_compare(h, &state.api_key),
        None => false,
    };

    if !authorized {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }

    if headers.get("mcp-session-id").is_some() {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }

    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost");
    let scheme = "http";
    let endpoint_url = format!("{}://{}/sse", scheme, host);

    let new_id = Uuid::new_v4().to_string();
    {
        let mut sessions = match state.sessions.write() {
            Ok(s) => s,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error: session state corrupted",
                )
                    .into_response()
            }
        };
        sessions.insert(
            new_id.clone(),
            Session {
                _id: new_id.clone(),
                _created_at: Instant::now(),
            },
        );
    }

    let endpoint_event = Event::default()
        .event("endpoint")
        .data(format!("{}?sessionId={}", endpoint_url, new_id));

    let pending = stream::pending::<Result<Event, std::convert::Infallible>>();
    let stream = stream::once(async { Ok(endpoint_event) }).chain(pending);

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

async fn handle_sse_delete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if state.rate_limiter.check().is_err() {
        return (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response();
    }

    let auth_header = headers
        .get("x-api-key")
        .or_else(|| headers.get("authorization"))
        .and_then(|h| h.to_str().ok());

    let authorized = match auth_header {
        Some(h) => safe_compare(h, &state.api_key),
        None => false,
    };

    if !authorized {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }

    if let Some(id_val) = headers.get("mcp-session-id") {
        if let Ok(id) = id_val.to_str() {
            let mut sessions = match state.sessions.write() {
                Ok(s) => s,
                Err(_) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Internal server error: session state corrupted",
                    )
                        .into_response()
                }
            };
            if sessions.remove(id).is_some() {
                return StatusCode::OK.into_response();
            }
        }
    }
    StatusCode::NOT_FOUND.into_response()
}

// --- Protocol Logic ---

async fn handle_protocol(req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let is_notification = req.id.is_none();

    let result = match req.method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "serverInfo": {
                "name": "meridian-mcp",
                "version": "0.2.0"
            }
        })),
        "notifications/initialized" => {
            info!("Client confirmed initialization.");
            return None;
        }
        "tools/list" => Ok(json!({
            "tools": [
                {
                    "name": "forensic_decomposition",
                    "description": "Perform Oaxaca-Blinder pay equity decomposition. reference_coefficients is required and names the counterfactual the headline is computed under. Per-level rows of categorical predictors in detailed_explained / detailed_unexplained are deviations from the pooled-sample share-weighted average of all levels (every level, the alphabetically first included); the constant is the entry named \"__ob_intercept__\" and is not a driver. result.run_metadata records the scheme, the normalisation convention and its level shares. result.support and result.warnings report how far the compared group's characteristics sit from the baseline group's and the residual degrees of freedom of each fitted regression; with quantile set, result.quantile_report holds the actual percentile gap, the RIF model total and the tie diagnostics.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
                            "categorical_predictors": { "type": "array", "items": { "type": "string" } },
                            "three_fold": { "type": "boolean" },
                            "quantile": { "type": "number" },
                            "reference_coefficients": { "type": "string", "enum": ["GroupA", "GroupB", "Pooled", "PooledNoIndicator", "Weighted"], "description": "Whose pay structure prices the characteristics gap. GroupB: the reference group's own coefficients (the compared group is priced as if paid under the reference group's pay structure). GroupA: the compared (non-reference) group's own coefficients. Pooled: one regression on both groups with a group indicator; the unexplained gap equals the indicator's coefficient (Jann 2008 pooled). PooledNoIndicator: one regression on both groups without an indicator (Neumark 1988, Stata omega). Weighted: the sample-share-weighted average of the two groups' coefficients (Cotton 1988). Exact, case-sensitive; any other value, or none, is an error." },
                            "bootstrap_reps": { "type": "integer" }
                        },
                        "required": ["csv_content", "outcome_variable", "group_variable", "reference_group", "predictors", "reference_coefficients"]
                    }
                },
                {
                    "name": "simulate_remediation",
                    "description": "Simulate budget allocation to fix identified pay gaps. target is Reference (the reference group's own pay line) or Pooled (the pooled line with a target-group indicator, the decomposition's Pooled line); the prediction interval, extrapolated flags and few_residual_df warning come from the same fit as the fair wage.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
                            "budget": { "type": "number" },
                            "target": { "type": "string", "enum": ["Reference", "Pooled"] },
                            "strategy": { "type": "string", "enum": ["Greedy", "Equitable"] },
                            "range_target": { "type": "string", "enum": ["Midpoint", "LowerBound", "UpperBound"] }
                        },
                        "required": ["csv_content", "outcome_variable", "group_variable", "reference_group", "predictors", "budget"]
                    }
                },
                {
                    "name": "verify_adjustments",
                    "description": "Validate a set of proposed wage adjustments by re-running the decomposition under reference_coefficients (required; same meaning as in forensic_decomposition).",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
                            "categorical_predictors": { "type": "array", "items": { "type": "string" } },
                            "reference_coefficients": { "type": "string", "enum": ["GroupA", "GroupB", "Pooled", "PooledNoIndicator", "Weighted"], "description": "Whose pay structure prices the characteristics gap. GroupB: the reference group's own coefficients (the compared group is priced as if paid under the reference group's pay structure). GroupA: the compared (non-reference) group's own coefficients. Pooled: one regression on both groups with a group indicator; the unexplained gap equals the indicator's coefficient (Jann 2008 pooled). PooledNoIndicator: one regression on both groups without an indicator (Neumark 1988, Stata omega). Weighted: the sample-share-weighted average of the two groups' coefficients (Cotton 1988). Exact, case-sensitive; any other value, or none, is an error." },
                            "adjustments": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "index": { "type": "integer" },
                                        "value": { "type": "number" }
                                    },
                                    "required": ["index", "value"]
                                }
                            }
                        },
                        "required": ["csv_content", "outcome_variable", "group_variable", "reference_group", "predictors", "reference_coefficients", "adjustments"]
                    }
                },
                {
                    "name": "check_defensibility",
                    "description": "Score each proposed adjustment against the 95% (or confidence_level) prediction range of comparable reference-group employees: the share of adjusted wages that land inside that range. Student t on the reference regression's residual degrees of freedom. Each row also says whether its fair wage extends the reference group's pay line beyond the range that group occupies (extrapolated). Predictor overrides are supported.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
                            "confidence_level": { "type": "number", "description": "Level of the prediction range, e.g. 0.90 (a fraction, not 90). A level outside 0.50-0.999 or not finite is refused with INVALID_CONFIDENCE_LEVEL; default 0.95." },
                            "adjustments": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "index": { "type": "integer" },
                                        "value": { "type": "number" },
                                        "predictor_overrides": { "type": "object", "additionalProperties": { "type": "string" } }
                                    },
                                    "required": ["index", "value"]
                                }
                            }
                        },
                        "required": ["csv_content", "outcome_variable", "group_variable", "reference_group", "predictors", "adjustments"]
                    }
                },
                {
                    "name": "generate_efficient_frontier",
                    "description": "Calculate the Efficient Frontier curve (Budget vs Statistical Significance). Each point carries the pooled regression's group coefficient, its Student t statistic and two-sided p-value on the pooled residual degrees of freedom.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
                            "confidence_level": { "type": "number", "description": "A point is significant when its p-value is below 1 minus this level. A fraction, not a percentage: a level outside 0.50-0.999 or not finite is refused with INVALID_CONFIDENCE_LEVEL; default 0.95. Each point echoes the level used as confidence_level." }
                        },
                        "required": ["csv_content", "outcome_variable", "group_variable", "reference_group", "predictors"]
                    }
                }
            ]
        })),
        "tools/call" => handle_tool_call(req.params).await,
        "ping" => Ok(json!({})),
        _ => {
            return Some(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(json!({
                    "code": -32601,
                    "message": format!("Method not found: {}", req.method)
                })),
                id: req.id,
            });
        }
    };

    if is_notification {
        if let Err(e) = result {
            error!("Error handling notification: {}", e);
        }
        return None;
    }

    match result {
        Ok(v) => Some(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(v),
            error: None,
            id: req.id,
        }),
        Err(e) => {
            let code = if e.to_string().starts_with("Method not found:") {
                -32601
            } else {
                -32603
            };
            Some(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(json!({
                    "code": code,
                    "message": e.to_string()
                })),
                id: req.id,
            })
        }
    }
}

async fn handle_tool_call(params: Option<Value>) -> Result<Value> {
    let mut params = params.ok_or_else(|| anyhow!("Missing params"))?;
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing tool name"))?
        .to_string();
    let arguments = match params.as_object_mut() {
        Some(map) => map
            .remove("arguments")
            .ok_or_else(|| anyhow!("Missing arguments"))?,
        None => return Err(anyhow!("Params must be an object")),
    };

    match name.as_str() {
        "forensic_decomposition" => {
            let mut mcp_params: McpDecompositionParams = serde_json::from_value(arguments)?;
            if let Some(reps) = mcp_params.bootstrap_reps {
                mcp_params.bootstrap_reps = Some(reps.min(10000));
            }
            let req = mcp_params.into();
            let res = tokio::task::spawn_blocking(move || decompose_inner(req))
                .await
                .map_err(|e| anyhow!(e))?
                .map_err(|e| anyhow!(e))?;
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string(&res)? }] }))
        }
        "simulate_remediation" => {
            let p: McpOptimizationParams = serde_json::from_value(arguments)?;
            let req = OptimizationRequest {
                csv_data: p.csv_content.into_bytes(),
                outcome_variable: p.outcome_variable,
                group_variable: p.group_variable,
                reference_group: p.reference_group,
                predictors: p.predictors,
                categorical_predictors: p.categorical_predictors,
                budget: p.budget,
                target_gap: p.target_gap,
                target: p.target.map(|s| match s.as_str() {
                    "Pooled" => OptimizationTarget::Pooled,
                    _ => OptimizationTarget::Reference,
                }),
                strategy: p.strategy.map(|s| match s.as_str() {
                    "Equitable" => AllocationStrategy::Equitable,
                    _ => AllocationStrategy::Greedy,
                }),
                min_gap_pct: p.min_gap_pct,
                forensic_mode: p.forensic_mode,
                adjust_both_groups: p.adjust_both_groups,
                confidence_level: p.confidence_level,
                range_target: p.range_target.map(|s| match s.as_str() {
                    "LowerBound" => RangeTarget::LowerBound,
                    "UpperBound" => RangeTarget::UpperBound,
                    _ => RangeTarget::Midpoint,
                }),
            };
            let res = tokio::task::spawn_blocking(move || optimize_inner(req))
                .await
                .map_err(|e| anyhow!(e))?
                .map_err(|e| anyhow!(e))?;
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string(&res)? }] }))
        }
        "verify_adjustments" => {
            let mut p: McpVerificationParams = serde_json::from_value(arguments)?;
            if let Some(reps) = p.decomposition_params.bootstrap_reps {
                p.decomposition_params.bootstrap_reps = Some(reps.min(10000));
            }
            let req = p.into();
            let res = tokio::task::spawn_blocking(move || verify_inner(req))
                .await
                .map_err(|e| anyhow!(e))?
                .map_err(|e| anyhow!(e))?;
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string(&res)? }] }))
        }
        "check_defensibility" => {
            let mut p: McpVerificationParams = serde_json::from_value(arguments)?;
            if let Some(reps) = p.decomposition_params.bootstrap_reps {
                p.decomposition_params.bootstrap_reps = Some(reps.min(10000));
            }
            let req = p.into();
            let res = tokio::task::spawn_blocking(move || check_defensibility_inner(req))
                .await
                .map_err(|e| anyhow!(e))?
                .map_err(|e| anyhow!(e))?;
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string(&res)? }] }))
        }
        "generate_efficient_frontier" => {
            let mut frontier_params: McpFrontierParams = serde_json::from_value(arguments)?;
            if let Some(reps) = frontier_params.decomposition_params.bootstrap_reps {
                frontier_params.decomposition_params.bootstrap_reps = Some(reps.min(10000));
            }
            let req = EfficientFrontierRequest {
                confidence_level: frontier_params.confidence_level,
                decomposition_params: frontier_params.decomposition_params.into(),
                steps: Some(50),
                max_budget: None,
            };
            let res = tokio::task::spawn_blocking(move || calculate_efficient_frontier_inner(req))
                .await
                .map_err(|e| anyhow!(e))?
                .map_err(|e| anyhow!(e))?;
            Ok(json!({ "content": [{ "type": "text", "text": serde_json::to_string(&res)? }] }))
        }
        _ => Err(anyhow!("Unknown tool: {}", name)),
    }
}

/// Constant-time comparison between a provided authorization header value and expected API key.
/// Accepts either direct API key string or "Bearer <API key>" format.
fn safe_compare(provided: &str, expected: &str) -> bool {
    let provided_bytes = provided.as_bytes();
    let expected_bytes = expected.as_bytes();

    let direct_match = provided_bytes.ct_eq(expected_bytes);

    let bearer_prefix = b"Bearer ";
    let bearer_match = if provided_bytes.starts_with(bearer_prefix) {
        let token_bytes = &provided_bytes[bearer_prefix.len()..];
        token_bytes.ct_eq(expected_bytes)
    } else {
        0.into()
    };

    (direct_match | bearer_match).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_compare() {
        let expected = "secret-api-key-12345";

        // Exact match
        assert!(safe_compare("secret-api-key-12345", expected));

        // Bearer prefix match
        assert!(safe_compare("Bearer secret-api-key-12345", expected));

        // Invalid keys
        assert!(!safe_compare("wrong-key", expected));
        assert!(!safe_compare("Bearer wrong-key", expected));
        assert!(!safe_compare("secret-api-key-1234", expected)); // shorter length
        assert!(!safe_compare("secret-api-key-123456", expected)); // longer length
        assert!(!safe_compare("", expected));
        assert!(!safe_compare("Bearer ", expected));
    }

    #[tokio::test]
    async fn test_handle_tool_call_missing_params() {
        let res = handle_tool_call(None).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Missing params");
    }

    #[tokio::test]
    async fn test_handle_tool_call_unknown_tool() {
        let params = json!({
            "name": "unknown_tool",
            "arguments": {}
        });
        let res = handle_tool_call(Some(params)).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().to_string(), "Unknown tool: unknown_tool");
    }

    #[tokio::test]
    async fn test_handle_protocol_tools_list() {
        let req = JsonRpcRequest {
            _jsonrpc: "2.0".to_string(),
            method: "tools/list".to_string(),
            params: None,
            id: Some(json!(1)),
        };
        let res = handle_protocol(req).await;
        assert!(res.is_some());
        let resp = res.unwrap();
        assert!(resp.result.is_some());
    }

    fn tool_schema(resp: JsonRpcResponse, name: &str) -> Value {
        resp.result.unwrap()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("tool {name} listed"))
            .clone()
    }

    // 0120-MERIDIAN S4 / T6: the scheme is required and exact, and the schema says so.
    #[tokio::test]
    async fn schema_requires_reference_coefficients_and_names_every_scheme() {
        for tool in ["forensic_decomposition", "verify_adjustments"] {
            let req = JsonRpcRequest {
                _jsonrpc: "2.0".to_string(),
                method: "tools/list".to_string(),
                params: None,
                id: Some(json!(1)),
            };
            let t = tool_schema(handle_protocol(req).await.unwrap(), tool);
            let required: Vec<&str> = t["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert!(
                required.contains(&"reference_coefficients"),
                "{tool} must require reference_coefficients"
            );
            let prop = &t["inputSchema"]["properties"]["reference_coefficients"];
            let enum_vals: Vec<&str> = prop["enum"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            let accepted = [
                "GroupA",
                "GroupB",
                "Pooled",
                "PooledNoIndicator",
                "Weighted",
            ];
            assert_eq!(enum_vals, accepted);
            let description = prop["description"].as_str().unwrap();
            for scheme in accepted {
                assert!(
                    description.contains(scheme),
                    "{tool}: the description must explain {scheme}"
                );
            }
        }
    }

    fn decomposition_args(scheme: Option<&str>) -> Value {
        let mut a = json!({
            "csv_content": "wage,x,gender\n10,1,F\n12,2,F\n11,3,F\n13,4,F\n15,5,F\n20,1,M\n22,2,M\n21,3,M\n23,4,M\n25,5,M\n",
            "outcome_variable": "wage",
            "group_variable": "gender",
            "reference_group": "F",
            "predictors": ["x"],
            "bootstrap_reps": 2
        });
        if let Some(s) = scheme {
            a["reference_coefficients"] = json!(s);
        }
        a
    }

    #[tokio::test]
    async fn a_missing_or_misspelt_scheme_is_refused_through_the_tool_call() {
        for bad in [None, Some("pooled"), Some("Neumark")] {
            for tool in ["forensic_decomposition"] {
                let res = handle_tool_call(Some(
                    json!({ "name": tool, "arguments": decomposition_args(bad) }),
                ))
                .await;
                let msg = res.unwrap_err().to_string();
                assert!(
                    msg.starts_with("UNKNOWN_REFERENCE_COEFFICIENTS"),
                    "{bad:?}: {msg}"
                );
            }
        }
        let ok = handle_tool_call(Some(json!({ "name": "forensic_decomposition", "arguments": decomposition_args(Some("PooledNoIndicator")) }))).await.unwrap();
        let text = ok["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            parsed["run_metadata"]["reference_coefficients_used"],
            "PooledNoIndicator"
        );
        assert_eq!(parsed["run_metadata"]["method"], "oaxaca-blinder-mean");
    }

    #[tokio::test]
    async fn test_handle_tool_call_missing_arguments() {
        let res = handle_tool_call(Some(json!({ "name": "forensic_decomposition" }))).await;
        assert_eq!(res.unwrap_err().to_string(), "Missing arguments");
    }
}
