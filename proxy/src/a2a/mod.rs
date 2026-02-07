pub mod agent_card;
pub mod http;
pub mod parser;

#[allow(dead_code)]
#[derive(Debug)]
pub enum A2aError {
    NotImplemented,
}

use crate::config::ProxyConfig;
use crate::grpc::GrpcClients;
use std::sync::Arc;

pub async fn run(_config: ProxyConfig, _clients: Arc<GrpcClients>) -> Result<(), A2aError> {
    // TODO: Implement HTTP + SSE interception for A2A.
    Ok(())
}
