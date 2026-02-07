use crate::config::ProxyConfig;
use crate::enforcement::{self, DecisionOutcome, ToolCallOwned};
use crate::grpc::GrpcClients;
use crate::mcp::parser::{parse_message, McpMessage};
use crate::policy_cache::PolicyCache;
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::Response;
use axum::routing::any;
use axum::Router;
use std::io;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

#[derive(Clone)]
struct McpState {
    config: ProxyConfig,
    clients: Arc<GrpcClients>,
    policy_cache: Arc<Mutex<PolicyCache>>,
    http_client: reqwest::Client,
}

pub fn router(
    config: ProxyConfig,
    clients: Arc<GrpcClients>,
    policy_cache: Arc<Mutex<PolicyCache>>,
) -> Router {
    let state = McpState {
        config,
        clients,
        policy_cache,
        http_client: reqwest::Client::new(),
    };
    Router::new()
        .route("/{*path}", any(proxy_mcp))
        .with_state(state)
}

#[tracing::instrument(
    name = "astragraph.proxy.request",
    skip(state, headers, body),
    fields(protocol = "mcp", path = %path)
)]
async fn proxy_mcp(
    State(state): State<McpState>,
    method: Method,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    if let Some(call) = parse_tool_call_from_jsonrpc(&method, &path, &body) {
        let circuit_breaker = crate::circuit_breaker::CircuitBreaker::new();
        let decision = enforcement::evaluate_tool_call(
            call,
            &state.config,
            &state.clients,
            &state.policy_cache,
            &circuit_breaker,
        )
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;

        match decision {
            DecisionOutcome::Allow => {}
            DecisionOutcome::Block(response) | DecisionOutcome::Queue(response) => {
                return Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .header("content-type", "application/json")
                    .body(Body::from(response))
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    }

    let upstream = forward_request(&state, method, path, headers, body).await?;
    build_intercepted_response(upstream).await
}

fn parse_tool_call_from_jsonrpc(method: &Method, path: &str, body: &[u8]) -> Option<ToolCallOwned> {
    if method != Method::POST {
        return None;
    }
    let payload: serde_json::Value = serde_json::from_slice(body).ok()?;
    if payload.get("jsonrpc")?.as_str()? != "2.0" {
        return None;
    }
    if payload.get("method")?.as_str()? != "tools/call" && !path.ends_with("tools/call") {
        return None;
    }
    let params = payload.get("params")?;
    let name = params.get("name")?.as_str()?.to_string();
    Some(ToolCallOwned {
        id: payload.get("id").cloned(),
        name,
        arguments: params.get("arguments").cloned(),
    })
}

async fn forward_request(
    state: &McpState,
    method: Method,
    path: String,
    headers: HeaderMap,
    body: Bytes,
) -> Result<reqwest::Response, StatusCode> {
    let url = format!(
        "{}/{}",
        state.config.http.mcp_upstream.trim_end_matches('/'),
        path
    );

    let mut request = state.http_client.request(method, url);
    for (name, value) in headers.iter() {
        if name.as_str().eq_ignore_ascii_case("host")
            || name.as_str().eq_ignore_ascii_case("content-length")
        {
            continue;
        }
        request = request.header(name, value);
    }

    request
        .body(body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)
}

async fn build_intercepted_response(upstream: reqwest::Response) -> Result<Response, StatusCode> {
    let status = upstream.status();
    let headers = upstream.headers().clone();
    let content_type = headers
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_sse = content_type.contains("text/event-stream");
    let is_json = content_type.contains("application/json");
    let (tx, rx) = mpsc::channel(32);

    tokio::spawn(async move {
        let mut stream = upstream.bytes_stream();
        let mut sse = SseInspector::default();
        let mut json_buffer = Vec::new();

        while let Some(next) = stream.next().await {
            match next {
                Ok(chunk) => {
                    if is_sse {
                        sse.push_chunk(&chunk, scan_mcp_data_frame);
                    } else if is_json {
                        json_buffer.extend_from_slice(&chunk);
                    }
                    if tx.send(Ok(chunk)).await.is_err() {
                        return;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(io::Error::other(err.to_string()))).await;
                    return;
                }
            }
        }

        if is_sse {
            sse.finish(scan_mcp_data_frame);
        } else if is_json {
            scan_mcp_json(&json_buffer);
        }
    });

    let mut response = Response::builder().status(status);
    for (name, value) in &headers {
        if name.as_str().eq_ignore_ascii_case("content-length") {
            continue;
        }
        response = response.header(name, value);
    }
    let body = Body::from_stream(ReceiverStream::new(rx));
    response.body(body).map_err(|_| StatusCode::BAD_GATEWAY)
}

#[derive(Default)]
struct SseInspector {
    pending: String,
    data_lines: Vec<String>,
}

impl SseInspector {
    fn push_chunk<F>(&mut self, chunk: &[u8], mut on_data: F)
    where
        F: FnMut(&str),
    {
        self.pending.push_str(&String::from_utf8_lossy(chunk));
        while let Some(newline) = self.pending.find('\n') {
            let mut line = self.pending[..newline].to_string();
            self.pending.drain(..=newline);
            if line.ends_with('\r') {
                line.pop();
            }
            self.push_line(&line, &mut on_data);
        }
    }

    fn finish<F>(&mut self, mut on_data: F)
    where
        F: FnMut(&str),
    {
        if !self.pending.trim().is_empty() {
            let line = self.pending.clone();
            self.pending.clear();
            self.push_line(line.trim_end_matches('\r'), &mut on_data);
        }
        self.flush_event(&mut on_data);
    }

    fn push_line<F>(&mut self, line: &str, on_data: &mut F)
    where
        F: FnMut(&str),
    {
        if let Some(value) = line.strip_prefix("data:") {
            self.data_lines.push(value.trim().to_string());
            return;
        }
        if line.trim().is_empty() {
            self.flush_event(on_data);
        }
    }

    fn flush_event<F>(&mut self, on_data: &mut F)
    where
        F: FnMut(&str),
    {
        for value in self.data_lines.drain(..) {
            on_data(&value);
        }
    }
}

fn scan_mcp_json(body: &[u8]) {
    if let Ok(McpMessage::ToolListResponse(response)) = parse_message(body) {
        for tool in response.tools {
            if let Some(description) = tool.description {
                if is_poisoning_suspected(&description) {
                    crate::telemetry::record_request("tool_poisoning_suspected", 0.0);
                }
            }
        }
    }
}

fn scan_mcp_data_frame(value: &str) {
    if let Ok(McpMessage::ToolListResponse(response)) = parse_message(value.as_bytes()) {
        for tool in response.tools {
            if let Some(description) = tool.description {
                if is_poisoning_suspected(&description) {
                    crate::telemetry::record_request("tool_poisoning_suspected", 0.0);
                }
            }
        }
    }
}

fn is_poisoning_suspected(description: &str) -> bool {
    let normalized = description.to_ascii_lowercase();
    let markers = [
        "ignore previous instructions",
        "system prompt",
        "override policy",
        "exfiltrate",
        "disable guardrail",
        "bypass safety",
    ];
    markers.iter().any(|marker| normalized.contains(marker))
}
