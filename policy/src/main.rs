mod alerting;
mod auth;
mod evaluator;
mod grpc;
mod parser;
mod store;
mod telemetry;
mod tls;
mod watcher;

use astragraph_proto::astragraph::policy_service_server::PolicyServiceServer;
use auth::AuthState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use parser::AgentPolicy;
use serde::Deserialize;
use serde_json::json;
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use store::{PolicyRolloutState, PolicyStore, PolicySummary, StoreError};
use tokio::net::TcpListener;
use tonic::transport::Server;

#[derive(Clone)]
struct AppState {
    store: Arc<RwLock<PolicyStore>>,
    auth: AuthState,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    telemetry::init();

    let store = Arc::new(RwLock::new(load_policies("policies")));
    let auth = AuthState::new();
    let state = AppState {
        store: store.clone(),
        auth,
    };

    let rest_app = Router::new()
        .route("/policies", get(list_policies))
        .route("/policies/:name", get(get_policy))
        .route("/policies/validate", post(validate_policy))
        .route("/policies/:name/history", get(get_policy_history))
        .route(
            "/policies/:name/rollout",
            get(get_policy_rollout)
                .post(start_policy_rollout)
                .delete(stop_policy_rollout),
        )
        .route("/policies/:name/rollout/promote", post(promote_policy_rollout))
        .route("/policies/:name/rollback", post(rollback_policy_rollout))
        .with_state(state.clone())
        .layer(middleware::from_fn(require_bearer));

    let rest_addr: SocketAddr = env::var("ASTRAGRAPH_POLICY_REST_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8081".to_string())
        .parse()?;
    let grpc_addr: SocketAddr = env::var("ASTRAGRAPH_POLICY_GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:9091".to_string())
        .parse()?;

    let tls_config = tls::server_tls_config()?;
    let grpc_service = grpc::PolicyServiceImpl::new(store.clone());

    let watcher_store = store.clone();
    let _watcher = watcher::start_watcher("policies", move |path| {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(policy) = parser::parse_policy(&content) {
                if let Ok(mut store) = watcher_store.write() {
                    store.insert(policy);
                }
            }
        }
    })
    .ok();

    let rest_listener = TcpListener::bind(rest_addr).await?;
    let rest_server = async move {
        axum::serve(rest_listener, rest_app)
            .await
            .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> { Box::new(err) })
    };

    let grpc_server = async move {
        Server::builder()
            .tls_config(tls_config)?
            .add_service(PolicyServiceServer::new(grpc_service))
            .serve(grpc_addr)
            .await
            .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> { Box::new(err) })
    };

    tokio::try_join!(rest_server, grpc_server)?;
    Ok(())
}

fn load_policies(dir: &str) -> PolicyStore {
    let mut store = PolicyStore::new();
    let path = PathBuf::from(dir);
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(policy) = parser::parse_policy(&content) {
                    store.insert(policy);
                }
            }
        }
    }
    store
}

async fn require_bearer(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = request.headers();
    if !has_bearer(headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(next.run(request).await)
}

fn has_bearer(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.starts_with("Bearer "))
        .unwrap_or(false)
}

async fn list_policies(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PolicySummary>>, StatusCode> {
    state
        .auth
        .ensure_role(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            &["read", "admin", "audit"],
        )
        .await
        .map_err(|err| match err {
            auth::AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            auth::AuthError::Forbidden => StatusCode::FORBIDDEN,
        })?;

    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(store.list()))
}

async fn get_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<PolicyWithHistoryResponse>, StatusCode> {
    state
        .auth
        .ensure_role(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            &["read", "admin", "audit"],
        )
        .await
        .map_err(|err| match err {
            auth::AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            auth::AuthError::Forbidden => StatusCode::FORBIDDEN,
        })?;

    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let policy = store.get(&name).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(PolicyWithHistoryResponse {
        policy: policy.clone(),
        history: store.history(&name),
        status: "active".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
struct ValidationRequest {
    yaml: String,
}

#[derive(Debug, Deserialize)]
struct StartRolloutRequest {
    yaml: String,
    percentage: u8,
}

async fn validate_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ValidationRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .auth
        .ensure_role(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            &["admin"],
        )
        .await
        .map_err(|err| match err {
            auth::AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            auth::AuthError::Forbidden => StatusCode::FORBIDDEN,
        })?;

    telemetry::record_evaluation("validate");
    match parser::parse_policy(&request.yaml) {
        Ok(_) => Ok(Json(json!({ "valid": true }))),
        Err(err) => Ok(Json(
            json!({ "valid": false, "error": format!("{:?}", err) }),
        )),
    }
}

async fn get_policy_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Vec<store::PolicyHistoryItem>>, StatusCode> {
    state
        .auth
        .ensure_role(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            &["read", "admin", "audit"],
        )
        .await
        .map_err(|err| match err {
            auth::AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            auth::AuthError::Forbidden => StatusCode::FORBIDDEN,
        })?;

    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(store.history(&name)))
}

async fn get_policy_rollout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Option<PolicyRolloutState>>, StatusCode> {
    state
        .auth
        .ensure_role(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            &["read", "admin", "audit"],
        )
        .await
        .map_err(|err| match err {
            auth::AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            auth::AuthError::Forbidden => StatusCode::FORBIDDEN,
        })?;

    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(store.rollout(&name)))
}

async fn start_policy_rollout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(request): Json<StartRolloutRequest>,
) -> Result<Json<PolicyRolloutState>, StatusCode> {
    state
        .auth
        .ensure_role(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            &["admin"],
        )
        .await
        .map_err(|err| match err {
            auth::AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            auth::AuthError::Forbidden => StatusCode::FORBIDDEN,
        })?;

    let candidate = match parser::parse_policy(&request.yaml) {
        Ok(policy) => policy,
        Err(_) => {
            telemetry::record_rollout_event(&name, "start", "failed");
            alerting::emit_rollout_event(
                "rollout_start_failed",
                &name,
                "warning",
                json!({"reason": "invalid_policy_yaml"}),
            )
            .await;
            return Err(StatusCode::BAD_REQUEST);
        }
    };
    let start_result: Result<(PolicyRolloutState, bool), StoreError> = {
        let mut store = state
            .store
            .write()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let had_active_rollout = store.rollout(&name).is_some();
        store
            .start_rollout(&name, candidate, request.percentage)
            .map(|rollout| (rollout, had_active_rollout))
    };
    let (rollout, had_active_rollout) = match start_result {
        Ok(value) => value,
        Err(err) => {
            telemetry::record_rollout_event(&name, "start", "failed");
            alerting::emit_rollout_event(
                "rollout_start_failed",
                &name,
                "warning",
                json!({"reason": format!("{:?}", err)}),
            )
            .await;
            return Err(map_store_err(err));
        }
    };
    telemetry::record_rollout_event(&name, "start", "success");
    if !had_active_rollout {
        telemetry::record_rollout_active(&name, 1);
    }
    alerting::emit_rollout_event(
        "rollout_start",
        &name,
        "info",
        json!({
            "percentage": rollout.percentage,
            "stable_version": rollout.stable_version,
            "candidate_version": rollout.candidate_version
        }),
    )
    .await;
    Ok(Json(rollout))
}

async fn stop_policy_rollout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .auth
        .ensure_role(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            &["admin"],
        )
        .await
        .map_err(|err| match err {
            auth::AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            auth::AuthError::Forbidden => StatusCode::FORBIDDEN,
        })?;

    let stopped = {
        let mut store = state
            .store
            .write()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        store.stop_rollout(&name)
    };
    if !stopped {
        telemetry::record_rollout_event(&name, "rollback", "failed");
        alerting::emit_rollout_event(
            "rollout_rollback_failed",
            &name,
            "warning",
            json!({"reason": "no_active_rollout"}),
        )
        .await;
        return Err(StatusCode::NOT_FOUND);
    }
    telemetry::record_rollout_event(&name, "rollback", "success");
    telemetry::record_rollout_active(&name, -1);
    alerting::emit_rollout_event(
        "rollout_rollback",
        &name,
        "warning",
        json!({"action": "rollback"}),
    )
    .await;
    Ok(Json(json!({
        "policy": name,
        "rollback": "completed",
        "active_rollout": false
    })))
}

async fn rollback_policy_rollout(
    state: State<AppState>,
    headers: HeaderMap,
    name: Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    stop_policy_rollout(state, headers, name).await
}

async fn promote_policy_rollout(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<PolicySummary>, StatusCode> {
    state
        .auth
        .ensure_role(
            headers.get("authorization").and_then(|v| v.to_str().ok()),
            &["admin"],
        )
        .await
        .map_err(|err| match err {
            auth::AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            auth::AuthError::Forbidden => StatusCode::FORBIDDEN,
        })?;

    let promote_result = {
        let mut store = state
            .store
            .write()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        store.promote_rollout(&name)
    };
    let promoted = match promote_result {
        Ok(value) => value,
        Err(err) => {
            telemetry::record_rollout_event(&name, "promote", "failed");
            alerting::emit_rollout_event(
                "rollout_promote_failed",
                &name,
                "warning",
                json!({"reason": format!("{:?}", err)}),
            )
            .await;
            return Err(map_store_err(err));
        }
    };
    telemetry::record_rollout_event(&name, "promote", "success");
    telemetry::record_rollout_active(&name, -1);
    alerting::emit_rollout_event(
        "rollout_promote",
        &name,
        "info",
        json!({"version": promoted.version}),
    )
    .await;
    Ok(Json(promoted))
}

fn map_store_err(err: StoreError) -> StatusCode {
    match err {
        StoreError::NotFound => StatusCode::NOT_FOUND,
        StoreError::InvalidInput(message) => {
            tracing::warn!("invalid rollout request: {message}");
            StatusCode::BAD_REQUEST
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct PolicyWithHistoryResponse {
    policy: AgentPolicy,
    history: Vec<store::PolicyHistoryItem>,
    status: String,
}
