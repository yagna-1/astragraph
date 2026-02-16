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

use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let config = config::ProxyConfig::load();
    telemetry::init();

    let clients = {
        let mut attempt: u64 = 1;
        loop {
            match grpc::GrpcClients::connect(&config).await {
                Ok(clients) => break Arc::new(clients),
                Err(err) => {
                    eprintln!(
                        "Failed to connect gRPC clients on attempt {attempt}; retrying: {err}"
                    );
                    attempt += 1;
                    sleep(Duration::from_secs(1)).await;
                }
            }
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
