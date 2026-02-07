use crate::a2a::agent_card::AgentCardCache;
use crate::a2a::parser::{parse_message, A2aMessage};
use crate::config::ProxyConfig;
use crate::enforcement::{self, DecisionOutcome, ToolCallOwned};
use crate::grpc::GrpcClients;
use crate::policy_cache::PolicyCache;
use astragraph_proto::astragraph::graph_node::Payload as GraphPayload;
use astragraph_proto::astragraph::{GraphNode, HandoffPayload, InsertNodeRequest, NodeType};
use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::Response;
use axum::routing::any;
use axum::Router;
use prost_types::Timestamp;
use serde_json::json;
use std::hash::{Hash, Hasher};
use std::io;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

#[derive(Clone)]
struct A2aState {
    config: ProxyConfig,
    clients: Arc<GrpcClients>,
    policy_cache: Arc<Mutex<PolicyCache>>,
    http_client: reqwest::Client,
    card_cache: AgentCardCache,
}

pub fn router(
    config: ProxyConfig,
    clients: Arc<GrpcClients>,
    policy_cache: Arc<Mutex<PolicyCache>>,
) -> Router {
    let state = A2aState {
        config,
        clients,
        policy_cache,
        http_client: reqwest::Client::new(),
        card_cache: AgentCardCache::new(),
    };
    Router::new()
        .route("/{*path}", any(proxy_a2a))
        .with_state(state)
}

#[tracing::instrument(
    name = "astragraph.proxy.request",
    skip(state, headers, body),
    fields(protocol = "a2a", path = %path)
)]
async fn proxy_a2a(
    State(state): State<A2aState>,
    method: Method,
    Path(path): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, StatusCode> {
    if is_task_send(&method, &path) {
        if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body) {
            if let Some(card_url) = resolve_agent_card_url(&state, &payload) {
                if let Ok(card) = state.http_client.get(card_url).send().await {
                    if let Ok(text) = card.text().await {
                        let agent_id = payload
                            .pointer("/target/agent_id")
                            .and_then(|v| v.as_str())
                            .or_else(|| payload.get("target_agent_id").and_then(|v| v.as_str()))
                            .unwrap_or("unknown");
                        let changed = state.card_cache.update(agent_id, &text);
                        if changed {
                            return Response::builder()
                                .status(StatusCode::FORBIDDEN)
                                .header("content-type", "application/json")
                                .body(Body::from(
                                    json!({
                                        "error": "AGENT_CARD_CHANGED",
                                        "agent_id": agent_id
                                    })
                                    .to_string(),
                                ))
                                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
                        }
                    }
                }
            }

            let call = ToolCallOwned {
                id: payload.get("id").cloned(),
                name: "a2a.tasks.send".to_string(),
                arguments: Some(payload.clone()),
            };
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
                DecisionOutcome::Allow => {
                    let _ = insert_handoff(&state, &payload).await;
                }
                DecisionOutcome::Block(response) | DecisionOutcome::Queue(response) => {
                    return Response::builder()
                        .status(StatusCode::FORBIDDEN)
                        .header("content-type", "application/json")
                        .body(Body::from(response))
                        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    }

    let upstream = forward_request(&state, method, path, headers, body).await?;
    build_intercepted_response(upstream).await
}

fn is_task_send(method: &Method, path: &str) -> bool {
    method == Method::POST && path.ends_with("tasks/send")
}

async fn insert_handoff(state: &A2aState, payload: &serde_json::Value) -> Result<(), ()> {
    let workflow_id = payload
        .get("workflow_id")
        .and_then(|v| v.as_str())
        .unwrap_or("a2a-default");
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("task-unknown");
    let target_agent = payload
        .get("target_agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("agent-unknown");

    let context_hash = hash_payload(payload);
    let node = GraphNode {
        id: format!("handoff-{task_id}"),
        r#type: NodeType::Handoff as i32,
        agent_id: state.config.agent_id.clone(),
        workflow_id: workflow_id.to_string(),
        ts: Some(now_timestamp()),
        payload: Some(GraphPayload::Handoff(HandoffPayload {
            source_agent_id: state.config.agent_id.clone(),
            target_agent_id: target_agent.to_string(),
            task_id: task_id.to_string(),
            context_hash,
        })),
    };

    let mut client = state.clients.graph.clone();
    client
        .insert_node(InsertNodeRequest { node: Some(node) })
        .await
        .map_err(|_| ())?;
    Ok(())
}

fn hash_payload(payload: &serde_json::Value) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    payload.to_string().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn now_timestamp() -> Timestamp {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    Timestamp {
        seconds: now.as_secs() as i64,
        nanos: now.subsec_nanos() as i32,
    }
}

async fn forward_request(
    state: &A2aState,
    method: Method,
    path: String,
    headers: HeaderMap,
    body: Bytes,
) -> Result<reqwest::Response, StatusCode> {
    let url = format!(
        "{}/{}",
        state.config.http.a2a_upstream.trim_end_matches('/'),
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
        let mut sse = A2aSseInspector::default();
        let mut json_buffer = Vec::new();

        while let Some(next) = stream.next().await {
            match next {
                Ok(chunk) => {
                    if is_sse {
                        sse.push_chunk(&chunk, scan_a2a_task_status_data);
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
            sse.finish(scan_a2a_task_status_data);
        } else if is_json {
            scan_a2a_task_status_json(&json_buffer);
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
struct A2aSseInspector {
    pending: String,
    event: String,
    data_lines: Vec<String>,
}

impl A2aSseInspector {
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
        if let Some(name) = line.strip_prefix("event:") {
            self.event = name.trim().to_string();
            return;
        }
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
        if self.event == "task_status" {
            for value in self.data_lines.drain(..) {
                on_data(&value);
            }
        } else {
            self.data_lines.clear();
        }
        self.event.clear();
    }
}

fn scan_a2a_task_status_json(body: &[u8]) {
    if let Ok(A2aMessage::TaskStatus(event)) = parse_message(body) {
        crate::telemetry::record_request(
            &format!(
                "a2a_task_status_{}",
                event.task.status.state.to_ascii_lowercase()
            ),
            0.0,
        );
    }
}

fn scan_a2a_task_status_data(data: &str) {
    if let Ok(A2aMessage::TaskStatus(event)) = parse_message(data.as_bytes()) {
        crate::telemetry::record_request(
            &format!(
                "a2a_task_status_{}",
                event.task.status.state.to_ascii_lowercase()
            ),
            0.0,
        );
    }
}

fn resolve_agent_card_url(state: &A2aState, payload: &serde_json::Value) -> Option<String> {
    if let Some(url) = payload.get("agent_card_url").and_then(|v| v.as_str()) {
        return Some(url.to_string());
    }
    if let Some(url) = payload
        .pointer("/target/agent_card_url")
        .and_then(|v| v.as_str())
    {
        return Some(url.to_string());
    }
    if let Some(base) = payload
        .pointer("/target/base_url")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("target_base_url").and_then(|v| v.as_str()))
    {
        return Some(format!(
            "{}/.well-known/agent-card.json",
            base.trim_end_matches('/')
        ));
    }
    Some(format!(
        "{}/.well-known/agent-card.json",
        state.config.http.a2a_upstream.trim_end_matches('/')
    ))
}
