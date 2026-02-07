use crate::a2a;
use crate::config::ProxyConfig;
use crate::grpc::GrpcClients;
use crate::mcp;
use crate::policy_cache::PolicyCache;
use axum::Router;
use std::sync::Arc;
use tokio::net::TcpListener;

#[allow(dead_code)]
#[derive(Debug)]
pub enum HttpError {
    Io(std::io::Error),
}

impl From<std::io::Error> for HttpError {
    fn from(err: std::io::Error) -> Self {
        HttpError::Io(err)
    }
}

pub async fn run_http(config: ProxyConfig, clients: Arc<GrpcClients>) -> Result<(), HttpError> {
    let policy_cache = Arc::new(tokio::sync::Mutex::new(PolicyCache::new(
        config.policy_cache_ttl_ms,
    )));

    let mcp_router = mcp::http::router(config.clone(), clients.clone(), policy_cache.clone());
    let a2a_router = a2a::http::router(config.clone(), clients.clone(), policy_cache);

    let app = Router::new()
        .nest("/mcp", mcp_router)
        .nest("/a2a", a2a_router);

    let listener = TcpListener::bind(&config.http.listen_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
