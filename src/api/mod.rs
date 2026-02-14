use crate::analysis;
use crate::analysis::opportunity::detect_decay_opportunities_with_flow;
use crate::analysis::queue::infer_queue_position;
use crate::analysis::threat::analyze_threats;
use crate::config::AppConfig;
use crate::models::{CohortFlow, DecayOpportunity, PoolQueuePosition, StakePoolId, ThreatProfile};
use crate::pools;
use crate::rpc::RpcClient;
use crate::snapshot::{compute_stake_flow_diffs, summarize_flow_pressure, SnapshotStore, StakeFlowDiff};
use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
struct ApiState {
    config: AppConfig,
    db_path: PathBuf,
    rpc: RpcClient,
    http: reqwest::Client,
}

impl ApiState {
    fn new(config: AppConfig) -> Self {
        let db_path = config.snapshot_db_path();
        let rpc = RpcClient::from_config(&config.rpc);
        let http = reqwest::Client::new();
        Self {
            config,
            db_path,
            rpc,
            http,
        }
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(err: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: err.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

pub async fn serve(config: AppConfig, port: u16) -> Result<()> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::OPTIONS])
        .allow_headers(Any);
    let state = Arc::new(ApiState::new(config));

    let app = Router::new()
        .route("/api/health", get(health_handler))
        .route("/api/docs", get(docs_handler))
        .route("/api/threats", get(threats_handler))
        .route("/api/opportunities", get(opportunities_handler))
        .route("/api/queue", get(queue_handler))
        .route("/api/cohorts", get(cohorts_handler))
        .layer(cors)
        .with_state(state);

    let bind_addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(%bind_addr, "starting stake-radar API server");
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("failed to bind API server on {bind_addr}"))?;
    axum::serve(listener, app)
        .await
        .context("API server terminated unexpectedly")?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ThreatsQuery {
    validator: Option<String>,
    epochs: Option<u32>,
    tier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpportunitiesQuery {
    epochs: Option<u32>,
    min_stake: Option<f64>,
    cause: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QueueQuery {
    validator: Option<String>,
    pool: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CohortsQuery {
    epochs: Option<u32>,
    from: Option<String>,
    to: Option<String>,
}

#[derive(Debug, Serialize)]
struct ThreatsResponse {
    validator: String,
    latest_epoch: u64,
    your_stake_sol: f64,
    threats: Vec<ThreatProfile>,
}

#[derive(Debug, Serialize)]
struct OpportunitiesResponse {
    epoch_from: u64,
    epoch_to: u64,
    opportunities: Vec<DecayOpportunity>,
    used_stake_flow_diffs: bool,
}

#[derive(Debug, Serialize)]
struct QueueResponse {
    validator: String,
    pool: String,
    epoch: u64,
    score_count: usize,
    position: Option<PoolQueuePosition>,
}

#[derive(Debug, Serialize)]
struct CohortsResponse {
    epoch_from: u64,
    epoch_to: u64,
    used_stake_flow_diffs: bool,
    flows: Vec<CohortFlow>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    timestamp: chrono::DateTime<chrono::Utc>,
    db_available: bool,
    latest_epoch: Option<u64>,
}

#[derive(Debug, Serialize)]
struct DocsResponse {
    name: &'static str,
    routes: Vec<RouteDoc>,
}

#[derive(Debug, Serialize)]
struct RouteDoc {
    method: &'static str,
    path: &'static str,
    description: &'static str,
}

async fn health_handler(
    State(state): State<Arc<ApiState>>,
) -> Result<Json<HealthResponse>, ApiError> {
    let (db_available, latest_epoch) = match SnapshotStore::open(&state.db_path) {
        Ok(store) => (true, store.latest_epoch().map_err(ApiError::internal)?),
        Err(_) => (false, None),
    };
    Ok(Json(HealthResponse {
        status: "ok",
        timestamp: Utc::now(),
        db_available,
        latest_epoch,
    }))
}

async fn docs_handler() -> Json<DocsResponse> {
    Json(DocsResponse {
        name: "stake-radar REST API",
        routes: vec![
            RouteDoc {
                method: "GET",
                path: "/api/health",
                description: "Basic health check",
            },
            RouteDoc {
                method: "GET",
                path: "/api/docs",
                description: "Lists available API routes",
            },
            RouteDoc {
                method: "GET",
                path: "/api/threats?validator=<pubkey>&epochs=<n>&tier=<csv>",
                description: "Returns threat assessment for a validator",
            },
            RouteDoc {
                method: "GET",
                path: "/api/opportunities?epochs=<n>&min_stake=<sol>&cause=<csv>",
                description: "Returns decay opportunities",
            },
            RouteDoc {
                method: "GET",
                path: "/api/queue?validator=<pubkey>&pool=<pool>",
                description: "Returns stake pool queue position",
            },
            RouteDoc {
                method: "GET",
                path: "/api/cohorts?epochs=<n>&from=<filter>&to=<filter>",
                description: "Returns cohort flow analysis",
            },
        ],
    })
}

async fn threats_handler(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<ThreatsQuery>,
) -> Result<Json<ThreatsResponse>, ApiError> {
    let validator = query
        .validator
        .or_else(|| state.config.validator.vote_pubkey.clone())
        .ok_or_else(|| ApiError::bad_request("validator query parameter is required"))?;
    let lookback = query
        .epochs
        .unwrap_or(state.config.analysis.lookback_epochs)
        .max(2);

    let store = SnapshotStore::open(&state.db_path).map_err(ApiError::internal)?;
    let (epochs, histories) = load_histories(&store, lookback).map_err(ApiError::internal)?;
    if epochs.is_empty() {
        return Err(ApiError::bad_request(
            "no snapshots available; run snapshot capture first",
        ));
    }
    let latest_epoch = *epochs.first().unwrap_or(&0);
    let your_stake = histories
        .get(&validator)
        .and_then(|history| history.last())
        .map(|snapshot| snapshot.activated_stake_sol)
        .unwrap_or_default();
    let mut threats = analyze_threats(
        &histories,
        &validator,
        state.config.analysis.threat_overtake_horizon_epochs,
    );
    if let Some(tier_filter) = query.tier {
        let tier_set = parse_csv_set(&tier_filter);
        threats.retain(|threat| tier_set.contains(&threat.threat_tier.to_string()));
    }

    Ok(Json(ThreatsResponse {
        validator,
        latest_epoch,
        your_stake_sol: your_stake,
        threats,
    }))
}

async fn opportunities_handler(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<OpportunitiesQuery>,
) -> Result<Json<OpportunitiesResponse>, ApiError> {
    let lookback = query
        .epochs
        .unwrap_or(state.config.analysis.lookback_epochs)
        .max(2);
    let min_stake = query
        .min_stake
        .unwrap_or(state.config.analysis.min_opportunity_stake_sol);

    let store = SnapshotStore::open(&state.db_path).map_err(ApiError::internal)?;
    let (epochs, histories) = load_histories(&store, lookback).map_err(ApiError::internal)?;
    if epochs.is_empty() {
        return Err(ApiError::bad_request(
            "no snapshots available; run snapshot capture first",
        ));
    }
    let epoch_from = *epochs.last().unwrap_or(&0);
    let epoch_to = *epochs.first().unwrap_or(&0);
    let flow_pressure = load_stake_flow_diffs_for_epochs(&store, epoch_from, epoch_to)
        .map_err(ApiError::internal)?
        .map(|diffs| summarize_flow_pressure(&diffs));
    let mut opportunities =
        detect_decay_opportunities_with_flow(&histories, min_stake, flow_pressure.as_ref());
    if let Some(cause_filter) = query.cause {
        let cause_set = parse_csv_set(&cause_filter);
        opportunities.retain(|opp| cause_set.contains(&opp.probable_cause.to_string()));
    }

    Ok(Json(OpportunitiesResponse {
        epoch_from,
        epoch_to,
        opportunities,
        used_stake_flow_diffs: flow_pressure.is_some(),
    }))
}

async fn queue_handler(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<QueueQuery>,
) -> Result<Json<QueueResponse>, ApiError> {
    let validator = query
        .validator
        .or_else(|| state.config.validator.vote_pubkey.clone())
        .ok_or_else(|| ApiError::bad_request("validator query parameter is required"))?;
    let pool_raw = query
        .pool
        .ok_or_else(|| ApiError::bad_request("pool query parameter is required"))?;
    let pool = pool_raw
        .parse::<StakePoolId>()
        .map_err(ApiError::bad_request)?;

    let mut store = SnapshotStore::open(&state.db_path).map_err(ApiError::internal)?;
    let epoch = state
        .rpc
        .get_epoch_info()
        .await
        .map(|info| info.epoch)
        .unwrap_or_else(|_| store.latest_epoch().ok().flatten().unwrap_or(0));
    let scores = pools::fetch_pool_scores(pool, &state.http)
        .await
        .map_err(ApiError::internal)?;
    let prior_rank = store
        .load_prior_queue_rank(pool, &validator, epoch)
        .map_err(ApiError::internal)?;
    let position = infer_queue_position(&validator, pool, &scores, prior_rank);
    store
        .insert_pool_rankings(epoch, pool, &scores)
        .map_err(ApiError::internal)?;

    Ok(Json(QueueResponse {
        validator,
        pool: pool.to_string(),
        epoch,
        score_count: scores.len(),
        position,
    }))
}

async fn cohorts_handler(
    State(state): State<Arc<ApiState>>,
    Query(query): Query<CohortsQuery>,
) -> Result<Json<CohortsResponse>, ApiError> {
    let lookback = query
        .epochs
        .unwrap_or(state.config.analysis.lookback_epochs)
        .max(2);
    let store = SnapshotStore::open(&state.db_path).map_err(ApiError::internal)?;
    let epochs = store.recent_epochs(lookback).map_err(ApiError::internal)?;
    if epochs.len() < 2 {
        return Err(ApiError::bad_request(
            "at least two snapshot epochs are required",
        ));
    }

    let epoch_from = *epochs.last().unwrap_or(&0);
    let epoch_to = *epochs.first().unwrap_or(&0);
    let from_snapshots = store
        .load_epoch_snapshots(epoch_from)
        .map_err(ApiError::internal)?;
    let to_snapshots = store
        .load_epoch_snapshots(epoch_to)
        .map_err(ApiError::internal)?;

    let (mut flows, used_stake_flow_diffs) =
        if let Some(stake_diffs) = load_stake_flow_diffs_for_epochs(&store, epoch_from, epoch_to)
            .map_err(ApiError::internal)?
        {
            (
                analysis::cohort::compute_cohort_flows_from_stake_diffs(
                    epoch_from,
                    epoch_to,
                    &from_snapshots,
                    &to_snapshots,
                    &stake_diffs,
                ),
                true,
            )
        } else {
            (
                analysis::cohort::compute_cohort_flows(
                    epoch_from,
                    epoch_to,
                    &from_snapshots,
                    &to_snapshots,
                ),
                false,
            )
        };

    if let Some(from_filter) = query.from {
        let from_filter = from_filter.to_ascii_lowercase();
        flows.retain(|flow| flow.from_cohort.to_string().contains(&from_filter));
    }
    if let Some(to_filter) = query.to {
        let to_filter = to_filter.to_ascii_lowercase();
        flows.retain(|flow| flow.to_cohort.to_string().contains(&to_filter));
    }

    Ok(Json(CohortsResponse {
        epoch_from,
        epoch_to,
        used_stake_flow_diffs,
        flows,
    }))
}

fn load_histories(
    store: &SnapshotStore,
    lookback_epochs: u32,
) -> Result<(
    Vec<u64>,
    HashMap<String, Vec<crate::models::ValidatorSnapshot>>,
)> {
    let epochs = store.recent_epochs(lookback_epochs)?;
    if epochs.is_empty() {
        return Ok((epochs, HashMap::new()));
    }
    let from_epoch = *epochs.last().expect("epochs not empty");
    let to_epoch = *epochs.first().expect("epochs not empty");
    let snapshots = store.load_snapshots_for_epoch_range(from_epoch, to_epoch)?;
    let histories = analysis::group_by_validator(&snapshots);
    Ok((epochs, histories))
}

fn load_stake_flow_diffs_for_epochs(
    store: &SnapshotStore,
    epoch_from: u64,
    epoch_to: u64,
) -> Result<Option<Vec<StakeFlowDiff>>> {
    let Some(stake_from_epoch) = store.latest_stake_epoch_at_or_before(epoch_from)? else {
        return Ok(None);
    };
    let Some(stake_to_epoch) = store.latest_stake_epoch_at_or_before(epoch_to)? else {
        return Ok(None);
    };
    if stake_from_epoch >= stake_to_epoch {
        return Ok(None);
    }

    let from_accounts = store.load_stake_accounts_for_epoch(stake_from_epoch)?;
    let to_accounts = store.load_stake_accounts_for_epoch(stake_to_epoch)?;
    if from_accounts.is_empty() || to_accounts.is_empty() {
        return Ok(None);
    }
    Ok(Some(compute_stake_flow_diffs(
        stake_from_epoch,
        stake_to_epoch,
        &from_accounts,
        &to_accounts,
    )))
}

fn parse_csv_set(raw: &str) -> HashSet<String> {
    raw.split(',')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect()
}
