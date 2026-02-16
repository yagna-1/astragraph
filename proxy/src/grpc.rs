use crate::config::ProxyConfig;
use crate::tls::client_tls_config;
use astragraph_proto::astragraph::graph_service_client::GraphServiceClient;
use astragraph_proto::astragraph::policy_service_client::PolicyServiceClient;
use astragraph_proto::astragraph::verifier_service_client::VerifierServiceClient;
use tonic::transport::{Channel, Endpoint};

pub struct GrpcClients {
    pub graph: GraphServiceClient<Channel>,
    pub policy: PolicyServiceClient<Channel>,
    pub verifier: VerifierServiceClient<Channel>,
}

impl GrpcClients {
    pub async fn connect(config: &ProxyConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let graph = connect_endpoint(&config.grpc.graph_addr, config).await?;
        let policy = connect_endpoint(&config.grpc.policy_addr, config).await?;
        let verifier = if config.verifier_required_at_startup {
            connect_endpoint(&config.grpc.verifier_addr, config).await?
        } else {
            connect_endpoint_lazy(&config.grpc.verifier_addr, config)?
        };

        Ok(Self {
            graph: GraphServiceClient::new(graph),
            policy: PolicyServiceClient::new(policy),
            verifier: VerifierServiceClient::new(verifier),
        })
    }
}

async fn connect_endpoint(
    address: &str,
    config: &ProxyConfig,
) -> Result<Channel, Box<dyn std::error::Error>> {
    let endpoint = build_endpoint(address, config)?;
    Ok(endpoint.connect().await?)
}

fn connect_endpoint_lazy(
    address: &str,
    config: &ProxyConfig,
) -> Result<Channel, Box<dyn std::error::Error>> {
    let endpoint = build_endpoint(address, config)?;
    Ok(endpoint.connect_lazy())
}

fn build_endpoint(
    address: &str,
    config: &ProxyConfig,
) -> Result<Endpoint, Box<dyn std::error::Error>> {
    let tls = client_tls_config(
        &config.tls.cert_path,
        &config.tls.key_path,
        &config.tls.ca_path,
    )?;
    let endpoint = Endpoint::from_shared(address.to_string())?.tls_config(tls)?;
    Ok(endpoint)
}
