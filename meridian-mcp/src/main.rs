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
use pay_equity_engine::defensibility::check_defensibility_on;
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

/// The three enum arguments of the remedy tools are EXACT and case-sensitive (0122-MERIDIAN T11,
/// REM-12). A misspelt `"equitable"` used to run Greedy and a misspelt `"pooled"` used to run
/// Reference, so a caller got a remedy they did not ask for, without a word.
fn parse_target(value: &str) -> Result<OptimizationTarget> {
    match value {
        "Reference" => Ok(OptimizationTarget::Reference),
        "Pooled" => Ok(OptimizationTarget::Pooled),
        other => Err(anyhow!(
            "UNKNOWN_TARGET: target={other:?}; give Reference or Pooled (exact, case-sensitive), \
             or leave it out for Reference"
        )),
    }
}

fn parse_strategy(value: &str) -> Result<AllocationStrategy> {
    match value {
        "Greedy" => Ok(AllocationStrategy::Greedy),
        "Equitable" => Ok(AllocationStrategy::Equitable),
        other => Err(anyhow!(
            "UNKNOWN_STRATEGY: strategy={other:?}; give Greedy or Equitable (exact, case-sensitive), \
             or leave it out for Greedy"
        )),
    }
}

fn parse_range_target(value: &str) -> Result<RangeTarget> {
    match value {
        "Midpoint" => Ok(RangeTarget::Midpoint),
        "LowerBound" => Ok(RangeTarget::LowerBound),
        "UpperBound" => Ok(RangeTarget::UpperBound),
        other => Err(anyhow!(
            "UNKNOWN_RANGE_TARGET: range_target={other:?}; give Midpoint, LowerBound or UpperBound \
             (exact, case-sensitive), or leave it out for Midpoint"
        )),
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
    /// The pay line the amounts are judged on: `Reference` (default) or `Pooled` (0120 review N8).
    #[serde(default)]
    pub target: Option<String>,
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
    /// The remedy the curve follows (0122-MERIDIAN T10): the same settings as `simulate_remediation`.
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub range_target: Option<String>,
    #[serde(default)]
    pub min_gap_pct: Option<f64>,
    #[serde(default)]
    pub adjust_both_groups: Option<bool>,
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
                    "description": "Cost a remedy. It raises each compared employee below the chosen pay line up to it, never above, within the budget; it is a scenario, not a payment schedule. strategy only decides who is paid first when the money falls short: Greedy pays the largest shortfalls first, Equitable pays every employee the same share of their own shortfall. target is Reference (the reference group's own pay line) or Pooled (the pooled line with a target-group indicator, the decomposition's Pooled line); the prediction interval, extrapolated flags and few_residual_df warning come from the same fit as the fair wage. Every gap in the result has one sign: the compared group's figure minus the line, negative while the group sits below it. target_gap derives the budget: the least that brings the compared group's mean gap to the pay line (same sign and scale as original_unexplained_gap) to that figure; the result says whether it was already met or out of reach (target_gap_reachable, best_reachable_gap, shortfall_to_target). new_unexplained_gap is the gap on the line REFITTED to the schedule's wages, so raising reference employees (adjust_both_groups) moves it. best_reachable_gap is the highest the gap gets as the budget grows. required_budget is the compared group's need to the chosen line and threshold (check_defensibility always reads the midpoint at threshold 0). closure is the share of the compared group's need paid; unfunded_amount, unfunded_count and threshold_excluded_count say who is still below the line and why. Enumerated arguments are exact and case-sensitive; an unknown value is refused with UNKNOWN_TARGET, UNKNOWN_STRATEGY or UNKNOWN_RANGE_TARGET.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
                            "categorical_predictors": { "type": "array", "items": { "type": "string" } },
                            "budget": { "type": "number", "description": "The most the remedy may spend in total, over every person paid. 0 means no cap: every eligible shortfall is paid in full. A negative or non-finite value is refused with INVALID_BUDGET." },
                            "target_gap": { "type": "number", "description": "The compared group's mean gap to the pay line the remedy should reach, on the sign and scale of original_unexplained_gap (dollars per compared employee, negative while below the line); it is not the raw total_gap. Leave it out for no target. A figure already met pays nothing; one out of reach pays up to the budget where the gap is highest, which is every eligible shortfall unless paying someone on the Pooled line widens the gap, and says so. Refused with adjust_both_groups (TARGET_GAP_WITH_REFERENCE_RAISES) and when not finite (INVALID_TARGET_GAP)." },
                            "target": { "type": "string", "enum": ["Reference", "Pooled"], "description": "The pay line the shortfalls are measured to. Default Reference." },
                            "strategy": { "type": "string", "enum": ["Greedy", "Equitable"], "description": "Who is paid first when the budget falls short. Default Greedy." },
                            "range_target": { "type": "string", "enum": ["Midpoint", "LowerBound", "UpperBound"], "description": "Pay each person up to the midpoint of the fair range or to its lower or upper bound. Applies to reference employees too when adjust_both_groups is on. Default Midpoint." },
                            "min_gap_pct": { "type": "number", "description": "Smallest shortfall worth paying, as a fraction of the employee's CURRENT pay (shortfall / current pay, not / fair pay): 0.02 is 2 %. Employees below the line by less are left out and counted in threshold_excluded_count. 0 or more (INVALID_MIN_GAP_PCT otherwise); default 0." },
                            "forensic_mode": { "type": "boolean", "description": "List every analysed employee, paid or not." },
                            "adjust_both_groups": { "type": "boolean", "description": "Also raise reference employees below the pay line. The line is refitted on the raised pay, so it moves; cost_target and cost_reference are reported apart and closure counts the compared group only." },
                            "confidence_level": { "type": "number", "description": "Level of the prediction range, a fraction (0.95), 0.50-0.999; default 0.95." }
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
                    "description": "Score each proposed adjustment against the 95% (or confidence_level) prediction range of comparable reference-group employees: the share of adjusted wages that land inside that range. Student t on the residual degrees of freedom of the fit the fair wage is read off: the reference regression (target Reference, default) or the pooled regression with a group indicator (target Pooled, the line the remedy priced against). Each row also says whether its fair wage extends the reference group's pay line beyond the range that group occupies (extrapolated), and where it sits against its range (range_position, range_position_before). position_counts counts every analysed compared employee below, inside and above their range before and after the schedule, a row the schedule does not name counting at adjustment 0; reference employees are in no count. group_test is the exact test of the compared group after the schedule (the pooled regression with a group indicator on the schedule's wages); it is always on the pooled line (group_test.line), so under target Reference its group_coefficient is not new_unexplained_gap, and the two can differ in sign. Gaps carry the sign of simulate_remediation: compared minus the line, negative while below. Predictor overrides are supported.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
                            "confidence_level": { "type": "number", "description": "Level of the prediction range, e.g. 0.90 (a fraction, not 90). A level outside 0.50-0.999 or not finite is refused with INVALID_CONFIDENCE_LEVEL; default 0.95." },
                            "target": { "type": "string", "enum": ["Reference", "Pooled"], "description": "The pay line the amounts are judged on: the one the remedy was priced against. Default Reference. Exact and case-sensitive (UNKNOWN_TARGET otherwise)." },
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
                    "description": "Calculate the Efficient Frontier curve (Budget vs Statistical Significance) of a remedy. The curve follows the remedy the other arguments describe (the same settings as simulate_remediation), and its budget axis ends at that remedy's full cost. Each point carries the pooled regression's group coefficient, its Student t statistic and two-sided p-value on the pooled residual degrees of freedom, on the wages the remedy's schedule produces at that budget.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "csv_content": { "type": "string" },
                            "outcome_variable": { "type": "string" },
                            "group_variable": { "type": "string" },
                            "reference_group": { "type": "string" },
                            "predictors": { "type": "array", "items": { "type": "string" } },
                            "confidence_level": { "type": "number", "description": "A point is significant when its p-value is below 1 minus this level. A fraction, not a percentage: a level outside 0.50-0.999 or not finite is refused with INVALID_CONFIDENCE_LEVEL; default 0.95. Each point echoes the level used as confidence_level. With range_target LowerBound or UpperBound it is also the level of the interval the remedy pays to." },
                            "categorical_predictors": { "type": "array", "items": { "type": "string" } },
                            "target": { "type": "string", "enum": ["Reference", "Pooled"], "description": "The pay line the remedy is measured to. Default Reference." },
                            "strategy": { "type": "string", "enum": ["Greedy", "Equitable"], "description": "Who is paid first when the budget falls short. Default Greedy." },
                            "range_target": { "type": "string", "enum": ["Midpoint", "LowerBound", "UpperBound"], "description": "Pay up to the midpoint of the fair range or to its lower or upper bound. Default Midpoint." },
                            "min_gap_pct": { "type": "number", "description": "Smallest shortfall worth paying, as a fraction of current pay; default 0." },
                            "adjust_both_groups": { "type": "boolean", "description": "Also raise reference employees below the line; the axis then ends at the cost of both groups." }
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
                target: p.target.as_deref().map(parse_target).transpose()?,
                strategy: p.strategy.as_deref().map(parse_strategy).transpose()?,
                min_gap_pct: p.min_gap_pct,
                forensic_mode: p.forensic_mode,
                adjust_both_groups: p.adjust_both_groups,
                confidence_level: p.confidence_level,
                range_target: p
                    .range_target
                    .as_deref()
                    .map(parse_range_target)
                    .transpose()?,
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
            let target = p
                .target
                .as_deref()
                .map(parse_target)
                .transpose()?
                .unwrap_or(OptimizationTarget::Reference);
            let req = p.into();
            let res = tokio::task::spawn_blocking(move || check_defensibility_on(req, &target))
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
            let target = frontier_params
                .target
                .as_deref()
                .map(parse_target)
                .transpose()?;
            let strategy = frontier_params
                .strategy
                .as_deref()
                .map(parse_strategy)
                .transpose()?;
            let range_target = frontier_params
                .range_target
                .as_deref()
                .map(parse_range_target)
                .transpose()?;
            let req = EfficientFrontierRequest {
                confidence_level: frontier_params.confidence_level,
                target,
                strategy,
                range_target,
                min_gap_pct: frontier_params.min_gap_pct,
                adjust_both_groups: frontier_params.adjust_both_groups,
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

    // ---- 0122-MERIDIAN V13 / T11: every enumerated argument of every remedy tool is exact ----

    fn remedy_roster() -> &'static str {
        "wage,x,gender\n10,1,F\n12,2,F\n11,3,F\n13,4,F\n15,5,F\n20,1,M\n22,2,M\n21,3,M\n23,4,M\n25,5,M\n"
    }

    fn remedy_args(tool: &str) -> Value {
        let mut a = json!({
            "csv_content": remedy_roster(),
            "outcome_variable": "wage",
            "group_variable": "gender",
            "reference_group": "M",
            "predictors": ["x"],
        });
        match tool {
            "simulate_remediation" => a["budget"] = json!(0),
            "check_defensibility" => a["adjustments"] = json!([{ "index": 0, "value": 100.0 }]),
            _ => {}
        }
        a
    }

    async fn tools_list() -> Value {
        let req = JsonRpcRequest {
            _jsonrpc: "2.0".to_string(),
            method: "tools/list".to_string(),
            params: None,
            id: Some(json!(1)),
        };
        handle_protocol(req).await.unwrap().result.unwrap()
    }

    #[tokio::test]
    async fn every_enumerated_argument_of_every_remedy_tool_is_exact() {
        let listed = tools_list().await;
        let mut checked = 0;
        for tool in [
            "simulate_remediation",
            "check_defensibility",
            "generate_efficient_frontier",
        ] {
            let schema = listed["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == tool)
                .unwrap_or_else(|| panic!("{tool} listed"))
                .clone();
            let props = schema["inputSchema"]["properties"].as_object().unwrap();
            for (name, prop) in props {
                let Some(values) = prop["enum"].as_array() else {
                    continue;
                };
                let values: Vec<&str> = values.iter().map(|v| v.as_str().unwrap()).collect();
                let code = format!("UNKNOWN_{}", name.to_uppercase());
                // Every accepted value runs.
                for v in &values {
                    let mut args = remedy_args(tool);
                    args[name.as_str()] = json!(v);
                    let res =
                        handle_tool_call(Some(json!({ "name": tool, "arguments": args }))).await;
                    assert!(
                        res.is_ok(),
                        "{tool}.{name}={v}: {:?}",
                        res.err().map(|e| e.to_string())
                    );
                    checked += 1;
                }
                // Anything else is refused by name: lower case, empty, nonsense.
                for bad in [values[0].to_lowercase(), String::new(), "Nope".to_string()] {
                    let mut args = remedy_args(tool);
                    args[name.as_str()] = json!(bad);
                    let res =
                        handle_tool_call(Some(json!({ "name": tool, "arguments": args }))).await;
                    let msg = res
                        .expect_err(&format!("{tool}.{name}={bad:?} must be refused"))
                        .to_string();
                    assert!(msg.starts_with(&code), "{tool}.{name}={bad:?}: {msg}");
                    checked += 1;
                }
            }
        }
        // simulate_remediation and generate_efficient_frontier: target (2 accepted + 3 refused),
        // strategy (2 + 3), range_target (3 + 3) = 16 each; check_defensibility: target = 5.
        assert_eq!(
            checked,
            16 + 16 + 5,
            "an enumerated argument was added or lost"
        );
    }

    #[tokio::test]
    async fn the_remedy_tools_list_every_field_they_accept() {
        let listed = tools_list().await;
        let props = |tool: &str| -> Vec<String> {
            listed["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == tool)
                .unwrap()["inputSchema"]["properties"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect()
        };
        let sim = props("simulate_remediation");
        for f in [
            "budget",
            "target_gap",
            "target",
            "strategy",
            "range_target",
            "min_gap_pct",
            "forensic_mode",
            "adjust_both_groups",
            "confidence_level",
            "categorical_predictors",
        ] {
            assert!(
                sim.contains(&f.to_string()),
                "simulate_remediation does not list {f}"
            );
        }
        let frontier = props("generate_efficient_frontier");
        for f in [
            "target",
            "strategy",
            "range_target",
            "min_gap_pct",
            "adjust_both_groups",
        ] {
            assert!(
                frontier.contains(&f.to_string()),
                "generate_efficient_frontier does not list {f}"
            );
        }
    }

    #[tokio::test]
    async fn the_remedy_description_says_what_the_amounts_do() {
        let listed = tools_list().await;
        let text = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "simulate_remediation")
            .unwrap()["description"]
            .as_str()
            .unwrap()
            .to_string();
        for phrase in [
            "raises each compared employee below the chosen pay line up to it, never above, within the budget",
            "Greedy pays the largest shortfalls first",
            "Equitable pays every employee the same share",
            "a scenario, not a payment schedule",
        ] {
            assert!(text.contains(phrase), "description lacks: {phrase}");
        }
        assert!(!text.contains("solver"), "the remedy is not a solver");
    }

    #[tokio::test]
    async fn money_the_rule_cannot_honour_is_refused_through_the_tool_call() {
        for (field, value, code) in [
            ("budget", json!(-1), "INVALID_BUDGET"),
            ("min_gap_pct", json!(-0.5), "INVALID_MIN_GAP_PCT"),
        ] {
            let mut args = remedy_args("simulate_remediation");
            args[field] = value;
            let res = handle_tool_call(Some(
                json!({ "name": "simulate_remediation", "arguments": args }),
            ))
            .await;
            let msg = res.unwrap_err().to_string();
            assert!(msg.starts_with(code), "{field}: {msg}");
        }
        let mut args = remedy_args("simulate_remediation");
        args["target_gap"] = json!(-100);
        args["adjust_both_groups"] = json!(true);
        let res = handle_tool_call(Some(
            json!({ "name": "simulate_remediation", "arguments": args }),
        ))
        .await;
        assert!(res
            .unwrap_err()
            .to_string()
            .starts_with("TARGET_GAP_WITH_REFERENCE_RAISES"));
        // A target gap reaches the engine: the result says whether it was reachable.
        let mut args = remedy_args("simulate_remediation");
        args["target_gap"] = json!(-1.0);
        let ok = handle_tool_call(Some(
            json!({ "name": "simulate_remediation", "arguments": args }),
        ))
        .await
        .unwrap();
        let parsed: Value =
            serde_json::from_str(ok["content"][0]["text"].as_str().unwrap()).unwrap();
        assert!(parsed["target_gap_reachable"].is_boolean());
        assert!(parsed["best_reachable_gap"].is_number());
    }
    // ---- 0122-MERIDIAN C-03 / C-04: a setting typed into a tool reaches the engine ----
    //
    // The enum test above proves a value is accepted or refused; it cannot see a value that is
    // parsed and then dropped on the way to the request. Each setting is tried ALONE against the
    // default call, on Fixture F (40 compared rows with shortfalls on a noisy line), and the
    // result must change.

    fn fixture_f_args(tool: &str, extra: Value) -> Value {
        let mut a = json!({
            "csv_content": include_str!("../../engine/tests/fixtures/0122-fixture-f-noisy.csv"),
            "outcome_variable": "Salary",
            "group_variable": "Gender",
            "reference_group": "Male",
            "predictors": ["Experience", "Level"],
        });
        if tool == "simulate_remediation" {
            a["budget"] = json!(25000);
        }
        for (k, v) in extra.as_object().unwrap() {
            a[k.as_str()] = v.clone();
        }
        a
    }

    async fn call_tool(tool: &str, extra: Value) -> Value {
        let res = handle_tool_call(Some(
            json!({ "name": tool, "arguments": fixture_f_args(tool, extra) }),
        ))
        .await
        .unwrap_or_else(|e| panic!("{tool}: {e}"));
        serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    /// What a frontier curve is, for comparing two of them: its last budget and every coefficient.
    fn curve_signature(curve: &Value) -> String {
        let pts = curve.as_array().unwrap();
        let coefficients: Vec<String> = pts
            .iter()
            .map(|p| format!("{:.6}", p["group_coefficient"].as_f64().unwrap()))
            .collect();
        format!(
            "{:.6} {}",
            pts.last().unwrap()["budget"].as_f64().unwrap(),
            coefficients.join(",")
        )
    }

    /// What a remedy is: its cost, its figures and every amount it pays.
    fn remedy_signature(r: &Value) -> String {
        let paid: Vec<String> = r["adjustments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| format!("{:.4}", a["adjustment"].as_f64().unwrap()))
            .collect();
        format!(
            "{:.6} {:.6} {:.6} {}",
            r["total_cost"].as_f64().unwrap(),
            r["new_unexplained_gap"].as_f64().unwrap(),
            r["best_reachable_gap"].as_f64().unwrap(),
            paid.join(",")
        )
    }

    #[tokio::test]
    async fn every_setting_of_the_frontier_tool_reaches_the_engine() {
        let base = call_tool("generate_efficient_frontier", json!({})).await;
        let base_sig = curve_signature(&base);
        let settings = [
            ("target", json!("Pooled")),
            ("strategy", json!("Equitable")),
            ("range_target", json!("UpperBound")),
            ("range_target", json!("LowerBound")),
            ("min_gap_pct", json!(0.05)),
            ("adjust_both_groups", json!(true)),
        ];
        for (field, value) in settings {
            let curve = call_tool(
                "generate_efficient_frontier",
                json!({ field: value.clone() }),
            )
            .await;
            assert_ne!(
                curve_signature(&curve),
                base_sig,
                "{field}={value} left the curve as the default"
            );
            // The curve ends where the same remedy, uncapped, stops spending.
            let remedy = call_tool(
                "simulate_remediation",
                json!({ field: value.clone(), "budget": 0 }),
            )
            .await;
            let last = curve.as_array().unwrap().last().unwrap()["budget"]
                .as_f64()
                .unwrap();
            let cost = remedy["total_cost"].as_f64().unwrap();
            assert!(
                (last - cost).abs() < 1e-6 * cost.max(1.0),
                "{field}={value}: the curve ends at {last} but the remedy costs {cost}"
            );
        }
        // The level sets the interval a bound is read from, not only the significance threshold.
        let lower = call_tool(
            "generate_efficient_frontier",
            json!({ "range_target": "LowerBound" }),
        )
        .await;
        let lower_80 = call_tool(
            "generate_efficient_frontier",
            json!({ "range_target": "LowerBound", "confidence_level": 0.80 }),
        )
        .await;
        assert_ne!(curve_signature(&lower), curve_signature(&lower_80));
        let remedy_80 = call_tool(
            "simulate_remediation",
            json!({ "range_target": "LowerBound", "confidence_level": 0.80, "budget": 0 }),
        )
        .await;
        let last = lower_80.as_array().unwrap().last().unwrap()["budget"]
            .as_f64()
            .unwrap();
        assert!((last - remedy_80["total_cost"].as_f64().unwrap()).abs() < 1e-6 * last);
    }

    #[tokio::test]
    async fn every_setting_of_the_remedy_tool_reaches_the_engine() {
        let base = remedy_signature(&call_tool("simulate_remediation", json!({})).await);
        let settings = [
            ("budget", json!(10000)),
            ("target_gap", json!(-800.0)),
            ("target", json!("Pooled")),
            ("strategy", json!("Equitable")),
            ("range_target", json!("UpperBound")),
            ("min_gap_pct", json!(0.05)),
            ("forensic_mode", json!(true)),
            ("adjust_both_groups", json!(true)),
        ];
        for (field, value) in settings {
            let r = call_tool("simulate_remediation", json!({ field: value.clone() })).await;
            assert_ne!(
                remedy_signature(&r),
                base,
                "{field}={value} left the remedy as the default"
            );
        }
        let lower = call_tool(
            "simulate_remediation",
            json!({ "range_target": "LowerBound", "budget": 0 }),
        )
        .await;
        let lower_80 = call_tool(
            "simulate_remediation",
            json!({ "range_target": "LowerBound", "confidence_level": 0.80, "budget": 0 }),
        )
        .await;
        assert_ne!(remedy_signature(&lower), remedy_signature(&lower_80));
    }
}
