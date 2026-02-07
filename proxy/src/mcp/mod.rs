pub mod http;
pub mod parser;
pub mod transport;

use crate::config::ProxyConfig;
use crate::grpc::GrpcClients;
use std::sync::Arc;

pub async fn run(
    config: ProxyConfig,
    clients: Arc<GrpcClients>,
) -> Result<(), transport::TransportError> {
    transport::run_stdio(config, clients).await
}
