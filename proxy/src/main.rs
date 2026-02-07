mod a2a;
mod circuit_breaker;
mod config;
mod enforcement;
mod grpc;
mod http;
mod mcp;
mod policy_cache;
mod telemetry;
mod thinking_trace;
mod tls;

#[tokio::main]
async fn main() {
    let config = config::ProxyConfig::load();
    telemetry::init();

    let clients = match grpc::GrpcClients::connect(&config).await {
        Ok(clients) => std::sync::Arc::new(clients),
        Err(err) => {
            eprintln!("Failed to connect gRPC clients: {err}");
            return;
        }
    };

    let mcp_task = {
        let config = config.clone();
        let clients = clients.clone();
        tokio::spawn(async move { mcp::run(config, clients).await })
    };
    let a2a_task = {
        let config = config.clone();
        let clients = clients.clone();
        tokio::spawn(async move { a2a::run(config, clients).await })
    };
    let http_task = {
        let config = config.clone();
        let clients = clients.clone();
        tokio::spawn(async move { http::run_http(config, clients).await })
    };

    let _ = tokio::join!(mcp_task, a2a_task, http_task);
}
