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
}

impl From<McpVerificationParams> for VerificationRequest {
    fn from(p: McpVerificationParams) -> Self {
        Self {
            decomposition_params: p.decomposition_params.into(),
            adjustments: p.adjustments.into_iter().map(|a| a.into()).collect(),
        }
    }
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

/// A constant-time byte slice comparison to prevent timing attacks.
fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

struct AppState {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    api_key: String,
    rate_limiter: Arc<governor::DefaultDirectRateLimiter>,
}

impl AppState {
    /// Constant-time authentication check for the API key.
    /// Supports both direct key matches and "Bearer <key>" format.
    fn is_authorized(&self, auth_header: Option<&str>) -> bool {
        let h = match auth_header {
            Some(h) => h,
            None => return false,
        };

        let direct_match = constant_time_compare(h.as_bytes(), self.api_key.as_bytes());

        let bearer_prefix = "Bearer ";
        let bearer_match = if let Some(stripped) = h.strip_prefix(bearer_prefix) {
            constant_time_compare(stripped.as_bytes(), self.api_key.as_bytes())
        } else {
            false
        };

        direct_match || bearer_match
    }
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

    if !state.is_authorized(auth_header) {
        return (StatusCode::UNAUTHORIZED, "Invalid API Key").into_response();
    }

    let response_opt = handle_protocol(req.clone()).await;

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

    if !state.is_authorized(auth_header) {
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

    if !state.is_authorized(auth_header) {
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
                "version": "0.1.0"
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
                    "description": "Perform Oaxaca-Blinder pay equity decomposition.",
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
                            "reference_coefficients": { "type": "string", "enum": ["Pooled", "GroupA", "GroupB", "Weighted"] },
                            "bootstrap_reps": { "type": "integer" }
                        },
                        "required": ["csv_content", "outcome_variable", "group_variable", "reference_group", "predictors"]
                    }
                },
                {
                    "name": "simulate_remediation",
                    "description": "Simulate budget allocation to fix identified pay gaps.",
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
                    "description": "Validate a set of proposed wage adjustments by re-running the decomposition.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
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
                        "required": ["csv_content", "outcome_variable", "group_variable", "reference_group", "predictors", "adjustments"]
                    }
                },
                {
                    "name": "check_defensibility",
                    "description": "Audit specific adjustments for legal/statistical defensibility with predictor overrides.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
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
                    "description": "Calculate the Efficient Frontier curve (Budget vs Statistical Significance).",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } }
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
    let params = params.ok_or_else(|| anyhow!("Missing params"))?;
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing tool name"))?;
    let arguments = params
        .get("arguments")
        .ok_or_else(|| anyhow!("Missing arguments"))?;

    match name {
        "forensic_decomposition" => {
            let mut mcp_params: McpDecompositionParams = serde_json::from_value(arguments.clone())?;
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
            let p: McpOptimizationParams = serde_json::from_value(arguments.clone())?;
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
            let mut p: McpVerificationParams = serde_json::from_value(arguments.clone())?;
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
            let mut p: McpVerificationParams = serde_json::from_value(arguments.clone())?;
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
            let mut mcp_params: McpDecompositionParams = serde_json::from_value(arguments.clone())?;
            if let Some(reps) = mcp_params.bootstrap_reps {
                mcp_params.bootstrap_reps = Some(reps.min(10000));
            }
            let req = EfficientFrontierRequest {
                decomposition_params: mcp_params.into(),
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
