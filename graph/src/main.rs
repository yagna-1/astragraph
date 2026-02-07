mod algorithms;
mod auth;
mod ccg;
mod grpc;
mod pii_scrubber;
mod store;
mod telemetry;
mod tls;

use astragraph_proto::astragraph::graph_service_server::GraphServiceServer;
use auth::AuthState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
    response::Response,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use store::{GraphResponse, GraphStore, GraphSummary, NodeFilter};
use tokio::net::TcpListener;
use tonic::transport::Server;

#[derive(Clone)]
struct AppState {
    store: Arc<RwLock<GraphStore>>,
    auth: AuthState,
}

#[derive(Debug, Deserialize)]
struct NodeFilterQuery {
    #[serde(rename = "type")]
    node_type: Option<String>,
    agent_id: Option<String>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GraphQuery {
    status: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    telemetry::init();

    let flush_dir = env::var("ASTRAGRAPH_GRAPH_FLUSH_DIR").unwrap_or_else(|_| "data/graphs".into());
    let ttl_secs = env::var("ASTRAGRAPH_GRAPH_TTL_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60 * 60 * 24);
    let flush_interval = env::var("ASTRAGRAPH_GRAPH_FLUSH_INTERVAL_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);

    let store = Arc::new(RwLock::new(GraphStore::new(
        PathBuf::from(flush_dir),
        Duration::from_secs(ttl_secs),
    )));

    let state = AppState {
        store: store.clone(),
        auth: AuthState::new(),
    };

    let rest_app = Router::new()
        .route("/graphs/:id", get(get_graph))
        .route("/graphs/:id/nodes", get(list_nodes))
        .route("/graphs/:id/drift-path/:node_id", get(get_drift_path))
        .route("/graphs", get(list_graphs))
        .route("/audit/violations", get(list_violations))
        .route("/audit/violations/:id", get(get_violation))
        .route("/audit/export", get(export_audit))
        .with_state(state)
        .layer(middleware::from_fn(require_bearer));

    let rest_addr: SocketAddr = env::var("ASTRAGRAPH_GRAPH_REST_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()?;

    let grpc_addr: SocketAddr = env::var("ASTRAGRAPH_GRAPH_GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:9090".to_string())
        .parse()?;

    let grpc_service = grpc::GraphServiceImpl::new(store.clone());
    let tls_config = tls::server_tls_config()?;

    let flush_store = store.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(flush_interval));
        loop {
            interval.tick().await;
            if let Ok(store) = flush_store.read() {
                let _ = store.flush_to_disk();
            }
            if let Ok(mut store) = flush_store.write() {
                store.cleanup_expired();
            }
        }
    });

    let rest_listener = TcpListener::bind(rest_addr).await?;
    let rest_server = async move {
        axum::serve(rest_listener, rest_app)
            .await
            .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> { Box::new(err) })
    };

    let grpc_server = async move {
        Server::builder()
            .tls_config(tls_config)?
            .add_service(GraphServiceServer::new(grpc_service))
            .serve(grpc_addr)
            .await
            .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> { Box::new(err) })
    };

    tokio::try_join!(rest_server, grpc_server)?;
    Ok(())
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

async fn get_graph(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<GraphResponse>, StatusCode> {
    ensure_roles(&state, &headers, &["read", "admin", "audit"]).await?;
    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let graph = store.get_graph(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(graph))
}

async fn list_nodes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(filter): Query<NodeFilterQuery>,
) -> Result<Json<Vec<store::GraphNodeResponse>>, StatusCode> {
    ensure_roles(&state, &headers, &["read", "admin", "audit"]).await?;
    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let filter = NodeFilter {
        node_type: filter.node_type,
        agent_id: filter.agent_id,
        status: filter.status,
    };
    let nodes = store
        .list_nodes(&id, &filter)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(nodes))
}

async fn get_drift_path(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((graph_id, node_id)): Path<(String, String)>,
) -> Result<Json<Vec<String>>, StatusCode> {
    ensure_roles(&state, &headers, &["read", "admin", "audit"]).await?;
    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let path = store
        .drift_path(&graph_id, &node_id, 0.7)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(path))
}

async fn list_graphs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GraphQuery>,
) -> Result<Json<Vec<GraphSummary>>, StatusCode> {
    ensure_roles(&state, &headers, &["read", "admin", "audit"]).await?;
    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut graphs = store.list_graphs(query.status.as_deref());
    graphs.sort_by(|left, right| left.graph_id.cmp(&right.graph_id));
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).min(1000);
    let page = graphs.into_iter().skip(offset).take(limit).collect();
    Ok(Json(page))
}

async fn list_violations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AuditViolationQuery>,
) -> Result<Json<Vec<store::ViolationRecord>>, StatusCode> {
    ensure_roles(&state, &headers, &["audit", "admin"]).await?;
    let Ok(store) = state.store.read() else {
        return Ok(Json(vec![]));
    };
    let mut records = store.violations();
    if let Some(agent_id) = query.agent_id {
        records.retain(|record| record.agent_id == agent_id);
    }
    if let Some(workflow_id) = query.workflow_id {
        records.retain(|record| record.workflow_id == workflow_id);
    }
    if let Some(rule_id) = query.rule_id {
        records.retain(|record| record.rule_id == rule_id);
    }
    if let Some(from_ts) = query.from_ts {
        records.retain(|record| record.timestamp >= from_ts);
    }
    if let Some(to_ts) = query.to_ts {
        records.retain(|record| record.timestamp <= to_ts);
    }
    Ok(Json(records))
}

async fn get_violation(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<store::ViolationRecord>, StatusCode> {
    ensure_roles(&state, &headers, &["audit", "admin"]).await?;
    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let record = store.violation(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(record))
}

async fn export_audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ExportQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    ensure_roles(&state, &headers, &["audit", "admin"]).await?;
    let store = state
        .store
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if query
        .format
        .as_deref()
        .is_some_and(|format| format.eq_ignore_ascii_case("json"))
    {
        return Ok((
            [(
                axum::http::header::CONTENT_TYPE,
                "application/json; charset=utf-8",
            )],
            serde_json::to_string(&store.violations()).unwrap_or_else(|_| "[]".to_string()),
        ));
    }
    Ok((
        [(axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8")],
        store.violations_csv(),
    ))
}

async fn ensure_roles(
    state: &AppState,
    headers: &HeaderMap,
    roles: &[&str],
) -> Result<(), StatusCode> {
    state
        .auth
        .ensure_role(
            headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            roles,
        )
        .await
        .map_err(|err| match err {
            auth::AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            auth::AuthError::Forbidden => StatusCode::FORBIDDEN,
        })
}

#[derive(Debug, Deserialize)]
struct AuditViolationQuery {
    agent_id: Option<String>,
    workflow_id: Option<String>,
    rule_id: Option<String>,
    from_ts: Option<i64>,
    to_ts: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ExportQuery {
    format: Option<String>,
}
