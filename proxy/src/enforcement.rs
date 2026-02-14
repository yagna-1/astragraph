use crate::circuit_breaker::{
    BlockResponse, BlockedAction, CircuitBreaker, Decision, FallbackMode,
};
use crate::config::ProxyConfig;
use crate::grpc::GrpcClients;
use crate::policy_cache::{CacheKey, PolicyCache, PolicyDecision};
use crate::thinking_trace::{self, TraceMode};
use astragraph_proto::astragraph::graph_node::Payload as GraphPayload;
use astragraph_proto::astragraph::{
    ActionPayload, ActionStatus, DriftPathRequest, Edge, EdgeType, GraphNode, InsertNodeRequest,
    LinkEdgeRequest, NodeType, PolicyEvaluationRequest, ThinkingPayload, VerificationPayload,
    VerifierRequest,
};
use prost_types::{value::Kind, ListValue, Struct, Timestamp, Value};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use tokio_stream::StreamExt;
use tracing::{info_span, Instrument};

static NODE_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct ToolCallOwned {
    pub id: Option<serde_json::Value>,
    pub name: String,
    pub arguments: Option<serde_json::Value>,
}

pub enum DecisionOutcome {
    Allow,
    Block(String),
    Queue(String),
}

pub async fn evaluate_tool_call(
    call: ToolCallOwned,
    config: &ProxyConfig,
    clients: &GrpcClients,
    policy_cache: &Mutex<PolicyCache>,
    circuit_breaker: &CircuitBreaker,
) -> Result<DecisionOutcome, String> {
    let decision_span = info_span!("astragraph.decision");
    let _decision_span_guard = decision_span.enter();
    let args_value = call.arguments.clone().unwrap_or_else(|| json!({}));
    let args_json = args_value.to_string();
    let now_utc = utc_hhmm();

    let trace_span = info_span!("astragraph.trace.extract");
    let extraction = {
        let _trace_span_guard = trace_span.enter();
        let content = args_value
            .get("thinking")
            .or_else(|| args_value.get("trace"))
            .or_else(|| args_value.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        thinking_trace::extract(parse_trace_mode(&config.trace_mode), content)
    };

    let workflow_id = workflow_id_from_id(&call.id);
    let action_id = format!("action-{}", next_id());
    let thinking_fut = async {
        if let Some(trace) = extraction.trace.clone() {
            let _ = insert_thinking_node(
                clients,
                &workflow_id,
                &format!("thinking-{}", next_id()),
                &config.agent_id,
                &trace,
            )
            .await;
        }
    };

    let cache_key = CacheKey {
        agent_id: &config.agent_id,
        tool_name: &call.name,
        args_json: &args_json,
    };
    let policy_fut = async {
        let decision = {
            let mut cache = policy_cache.lock().await;
            cache.get(&cache_key)
        };
        match decision {
            Some(decision) => Ok(decision),
            None => {
                let mut payload = args_value.clone();
                if let serde_json::Value::Object(map) = &mut payload {
                    map.insert(
                        "__now_utc".to_string(),
                        serde_json::Value::String(now_utc.clone()),
                    );
                }
                let mut client = clients.policy.clone();
                let request = PolicyEvaluationRequest {
                    policy_id: config.policy_id.clone(),
                    agent_id: config.agent_id.clone(),
                    tool_name: call.name.clone(),
                    arguments: Some(json_to_struct(&payload)),
                };
                let response = async {
                    client
                        .evaluate_action(request)
                        .await
                        .map_err(|_| ())
                        .map(|value| value.into_inner())
                }
                .instrument(info_span!("astragraph.policy.evaluate"))
                .await?;
                let decision = PolicyDecision {
                    allowed: response.allowed,
                    rule_id: response.rule_id,
                    threshold: response.threshold,
                    fallback: response.fallback,
                    require_confirmation: response.require_confirmation,
                };
                let mut cache = policy_cache.lock().await;
                cache.insert(&cache_key, decision.clone());
                Ok(decision)
            }
        }
    };
    let (policy_result, _) = tokio::join!(policy_fut, thinking_fut);
    let policy_decision = match policy_result {
        Ok(result) => result,
        Err(()) => {
            if config.fail_closed {
                return Err(block_with_fallback(call, config, None, "policy_error"));
            }
            return Ok(DecisionOutcome::Allow);
        }
    };

    if !policy_decision.allowed {
        let mut blocked_args = args_value.clone();
        if let serde_json::Value::Object(map) = &mut blocked_args {
            map.insert(
                "__policy_rule_id".to_string(),
                serde_json::Value::String(policy_decision.rule_id.clone()),
            );
        }
        let _ = insert_action_node(
            clients,
            &workflow_id,
            &action_id,
            &config.agent_id,
            &call.name,
            &blocked_args,
            ActionStatus::Blocked,
        )
        .await;
        let response = block_response(call, config, &policy_decision, None, 1.0).await;
        return Ok(DecisionOutcome::Block(response));
    }

    let has_thinking_trace = extraction.trace.is_some();
    let verifier_span = info_span!("astragraph.verifier.score");
    let verifier_score = {
        let _verifier_span_guard = verifier_span.enter();
        let reasoning = extraction.trace.clone().unwrap_or_default();
        let action = extraction
            .action
            .unwrap_or_else(|| format!("{} {}", call.name, args_json));
        score_with_retry(
            clients,
            VerifierRequest {
                policy_text: build_policy_text(config, &policy_decision),
                agent_reasoning: reasoning,
                agent_action: action,
            },
            map_fallback(policy_decision.fallback),
        )
        .await
    };
    if verifier_score.is_none() {
        crate::telemetry::record_false_abstention();
    }

    let score = verifier_score.as_ref().map(|resp| resp.deviation_score);
    let fallback = map_fallback(policy_decision.fallback);
    let allow_without_trace = config
        .allow_without_trace_tools
        .iter()
        .any(|tool| tool == &call.name);

    let decision = circuit_breaker.decide(
        policy_decision.allowed,
        has_thinking_trace,
        allow_without_trace,
        score,
        policy_decision.threshold,
        fallback,
        true,
    );

    let action_status = match decision {
        Decision::Allow => ActionStatus::Allowed,
        Decision::Block | Decision::Queue => ActionStatus::Blocked,
    };

    let _ = insert_action_node(
        clients,
        &workflow_id,
        &action_id,
        &config.agent_id,
        &call.name,
        &args_value,
        action_status,
    )
    .await;

    if let Some(score_resp) = verifier_score.as_ref() {
        let verification_id = format!("verification-{}", next_id());
        let _ = insert_verification_node(
            clients,
            &workflow_id,
            &verification_id,
            &config.agent_id,
            &action_id,
            score_resp.deviation_score,
            &config.policy_id,
            action_status,
            &score_resp.verifier_model,
            score_resp.latency_ms,
            &score_resp.verifier_thinking,
        )
        .await;
        let _ = link_verification_edge(clients, &action_id, &verification_id).await;
    }

    let drift_origin = if matches!(decision, Decision::Block) {
        drift_path_origin(clients, &workflow_id, &action_id, policy_decision.threshold).await
    } else {
        None
    };

    match decision {
        Decision::Allow => Ok(DecisionOutcome::Allow),
        Decision::Queue => Ok(DecisionOutcome::Queue(queue_response(call))),
        Decision::Block => Ok(DecisionOutcome::Block(
            block_response(
                call,
                config,
                &policy_decision,
                drift_origin,
                score.unwrap_or(1.0),
            )
            .await,
        )),
    }
}

async fn insert_action_node(
    clients: &GrpcClients,
    workflow_id: &str,
    node_id: &str,
    agent_id: &str,
    tool_name: &str,
    arguments: &serde_json::Value,
    status: ActionStatus,
) -> Result<(), ()> {
    async {
        let node = GraphNode {
            id: node_id.to_string(),
            r#type: NodeType::Action as i32,
            agent_id: agent_id.to_string(),
            workflow_id: workflow_id.to_string(),
            ts: Some(now_timestamp()),
            payload: Some(GraphPayload::Action(ActionPayload {
                action_type: "tool_call".to_string(),
                tool_name: tool_name.to_string(),
                arguments: Some(json_to_struct(arguments)),
                status: status as i32,
            })),
        };
        let mut client = clients.graph.clone();
        let request_stream = tokio_stream::iter(vec![InsertNodeRequest { node: Some(node) }]);
        let mut response_stream = client
            .stream_nodes(request_stream)
            .await
            .map_err(|_| ())?
            .into_inner();
        let _ = response_stream.next().await;
        Ok(())
    }
    .instrument(info_span!("astragraph.graph.insert"))
    .await
}

#[allow(clippy::too_many_arguments)]
async fn insert_verification_node(
    clients: &GrpcClients,
    workflow_id: &str,
    node_id: &str,
    agent_id: &str,
    parent_node_id: &str,
    deviation_score: f32,
    policy_id: &str,
    verdict: ActionStatus,
    verifier_model: &str,
    latency_ms: u32,
    verifier_thinking: &str,
) -> Result<(), ()> {
    async {
        let node = GraphNode {
            id: node_id.to_string(),
            r#type: NodeType::Verification as i32,
            agent_id: agent_id.to_string(),
            workflow_id: workflow_id.to_string(),
            ts: Some(now_timestamp()),
            payload: Some(GraphPayload::Verification(VerificationPayload {
                parent_node_id: parent_node_id.to_string(),
                deviation_score,
                policy_id: policy_id.to_string(),
                verdict: verdict as i32,
                verifier_model: verifier_model.to_string(),
                latency_ms,
                verifier_thinking: verifier_thinking.to_string(),
            })),
        };
        let mut client = clients.graph.clone();
        let request_stream = tokio_stream::iter(vec![InsertNodeRequest { node: Some(node) }]);
        let mut response_stream = client
            .stream_nodes(request_stream)
            .await
            .map_err(|_| ())?
            .into_inner();
        let _ = response_stream.next().await;
        Ok(())
    }
    .instrument(info_span!("astragraph.graph.insert"))
    .await
}

async fn link_verification_edge(
    clients: &GrpcClients,
    from_id: &str,
    to_id: &str,
) -> Result<(), ()> {
    async {
        let edge = Edge {
            from: from_id.to_string(),
            to: to_id.to_string(),
            r#type: EdgeType::VerifiedBy as i32,
        };
        let mut client = clients.graph.clone();
        let request_stream = tokio_stream::iter(vec![LinkEdgeRequest { edge: Some(edge) }]);
        let mut response_stream = client
            .stream_edges(request_stream)
            .await
            .map_err(|_| ())?
            .into_inner();
        let _ = response_stream.next().await;
        Ok(())
    }
    .instrument(info_span!("astragraph.graph.insert"))
    .await
}

async fn drift_path_origin(
    clients: &GrpcClients,
    graph_id: &str,
    node_id: &str,
    threshold: f32,
) -> Option<String> {
    let mut client = clients.graph.clone();
    let response = client
        .get_drift_path(DriftPathRequest {
            graph_id: graph_id.to_string(),
            node_id: node_id.to_string(),
            threshold,
        })
        .await
        .ok()?;
    Some(response.into_inner().origin)
}

async fn score_with_retry(
    clients: &GrpcClients,
    request: VerifierRequest,
    fallback: FallbackMode,
) -> Option<astragraph_proto::astragraph::VerifierResponse> {
    let mut client = clients.verifier.clone();
    let stream_request = tokio_stream::iter(vec![request.clone()]);
    if let Ok(response) = client.stream_score(stream_request).await {
        let mut stream = response.into_inner();
        if let Some(Ok(message)) = stream.next().await {
            crate::telemetry::record_verification_latency(message.latency_ms as f64);
            return Some(message);
        }
    }

    if !matches!(fallback, FallbackMode::Queue) {
        return None;
    }

    sleep(Duration::from_millis(500)).await;
    let mut retry_client = clients.verifier.clone();
    let retry_stream_request = tokio_stream::iter(vec![request]);
    let response = retry_client.stream_score(retry_stream_request).await.ok()?;
    let mut stream = response.into_inner();
    stream.next().await.and_then(|message| {
        message.ok().inspect(|value| {
            crate::telemetry::record_verification_latency(value.latency_ms as f64);
        })
    })
}

async fn insert_thinking_node(
    clients: &GrpcClients,
    workflow_id: &str,
    node_id: &str,
    agent_id: &str,
    reasoning: &str,
) -> Result<(), ()> {
    async {
        let node = GraphNode {
            id: node_id.to_string(),
            r#type: NodeType::Thinking as i32,
            agent_id: agent_id.to_string(),
            workflow_id: workflow_id.to_string(),
            ts: Some(now_timestamp()),
            payload: Some(GraphPayload::Thinking(ThinkingPayload {
                content: reasoning.to_string(),
                model_name: "agent".to_string(),
                token_count: reasoning.split_whitespace().count() as u32,
                trace_id: format!("trace-{}", next_id()),
            })),
        };
        let mut client = clients.graph.clone();
        let request_stream = tokio_stream::iter(vec![InsertNodeRequest { node: Some(node) }]);
        let mut response_stream = client
            .stream_nodes(request_stream)
            .await
            .map_err(|_| ())?
            .into_inner();
        let _ = response_stream.next().await;
        Ok(())
    }
    .instrument(info_span!("astragraph.graph.insert"))
    .await
}

async fn block_response(
    call: ToolCallOwned,
    config: &ProxyConfig,
    decision: &PolicyDecision,
    drift_origin: Option<String>,
    deviation_score: f32,
) -> String {
    crate::telemetry::record_violation(&decision.rule_id);
    let blocked_action = BlockedAction {
        tool: call.name,
        args: call.arguments.unwrap_or_else(|| json!({})),
    };
    let response = BlockResponse {
        error: "POLICY_VIOLATION".to_string(),
        violation_id: format!("violation-{}", next_id()),
        rule_id: decision.rule_id.clone(),
        deviation_score,
        threshold: decision.threshold,
        drift_path_origin: drift_origin.unwrap_or_default(),
        agent_id: config.agent_id.clone(),
        blocked_action,
        timestamp: iso8601_now(),
    };
    jsonrpc_error(call.id, response)
}

fn block_with_fallback(
    call: ToolCallOwned,
    config: &ProxyConfig,
    decision: Option<&PolicyDecision>,
    reason: &str,
) -> String {
    let decision = decision.cloned().unwrap_or(PolicyDecision {
        allowed: false,
        rule_id: "policy_error".to_string(),
        threshold: 1.0,
        fallback: astragraph_proto::astragraph::FallbackMode::FallbackBlock as i32,
        require_confirmation: false,
    });
    let response = BlockResponse {
        error: format!("POLICY_ERROR_{reason}"),
        violation_id: format!("violation-{}", next_id()),
        rule_id: decision.rule_id.clone(),
        deviation_score: 1.0,
        threshold: decision.threshold,
        drift_path_origin: "".to_string(),
        agent_id: config.agent_id.clone(),
        blocked_action: BlockedAction {
            tool: call.name,
            args: call.arguments.unwrap_or_else(|| json!({})),
        },
        timestamp: iso8601_now(),
    };
    jsonrpc_error(call.id, response)
}

fn queue_response(call: ToolCallOwned) -> String {
    let error = json!({
        "code": 503,
        "message": "QUEUE",
        "data": { "detail": "queued for verification" }
    });
    jsonrpc_error(call.id, error)
}

fn jsonrpc_error(id: Option<serde_json::Value>, data: impl serde::Serialize) -> String {
    let payload = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": 403,
            "message": "POLICY_VIOLATION",
            "data": data
        }
    });
    payload.to_string()
}

fn map_fallback(fallback: i32) -> FallbackMode {
    match astragraph_proto::astragraph::FallbackMode::try_from(fallback) {
        Ok(astragraph_proto::astragraph::FallbackMode::FallbackAllow) => FallbackMode::Allow,
        Ok(astragraph_proto::astragraph::FallbackMode::FallbackBlock) => FallbackMode::Block,
        Ok(astragraph_proto::astragraph::FallbackMode::FallbackQueue) => FallbackMode::Queue,
        Err(_) => FallbackMode::Block,
    }
}

fn workflow_id_from_id(id: &Option<serde_json::Value>) -> String {
    id.as_ref()
        .and_then(|id| id.as_str().map(|value| value.to_string()))
        .unwrap_or_else(|| "mcp-default".to_string())
}

fn build_policy_text(config: &ProxyConfig, decision: &PolicyDecision) -> String {
    format!(
        "policy_id: {}\nrule_id: {}\nthreshold: {}\nfallback: {}\nrequire_confirmation: {}",
        config.policy_id,
        decision.rule_id,
        decision.threshold,
        decision.fallback,
        decision.require_confirmation
    )
}

fn parse_trace_mode(raw: &str) -> TraceMode {
    match raw.to_ascii_lowercase().as_str() {
        "explicit" => TraceMode::Explicit,
        "streaming" => TraceMode::Streaming,
        _ => TraceMode::Absent,
    }
}

fn utc_hhmm() -> String {
    let now = chrono_now();
    let hours = (now / 3600) % 24;
    let minutes = (now / 60) % 60;
    format!("{hours:02}:{minutes:02}")
}

fn next_id() -> u64 {
    NODE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn chrono_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

fn iso8601_now() -> String {
    chrono::Utc::now().to_rfc3339()
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

fn json_to_struct(value: &serde_json::Value) -> Struct {
    let fields = match value {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(key, value)| (key.clone(), json_to_value(value)))
            .collect(),
        _ => Default::default(),
    };
    Struct { fields }
}

fn json_to_value(value: &serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value {
            kind: Some(Kind::NullValue(0)),
        },
        serde_json::Value::Bool(value) => Value {
            kind: Some(Kind::BoolValue(*value)),
        },
        serde_json::Value::Number(value) => Value {
            kind: Some(Kind::NumberValue(value.as_f64().unwrap_or(0.0))),
        },
        serde_json::Value::String(value) => Value {
            kind: Some(Kind::StringValue(value.clone())),
        },
        serde_json::Value::Array(values) => Value {
            kind: Some(Kind::ListValue(ListValue {
                values: values.iter().map(json_to_value).collect(),
            })),
        },
        serde_json::Value::Object(values) => Value {
            kind: Some(Kind::StructValue(Struct {
                fields: values
                    .iter()
                    .map(|(key, value)| (key.clone(), json_to_value(value)))
                    .collect(),
            })),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{queue_response, ToolCallOwned};
    use serde_json::json;

    #[test]
    fn queue_response_wraps_queue_detail_in_policy_violation_envelope() {
        let payload = queue_response(ToolCallOwned {
            id: Some(json!("req-queue-1")),
            name: "safe_tool".to_string(),
            arguments: Some(json!({"foo":"bar"})),
        });

        let parsed: serde_json::Value = serde_json::from_str(&payload).expect("json response");
        assert_eq!(parsed["error"]["code"], 403);
        assert_eq!(parsed["error"]["message"], "POLICY_VIOLATION");
        assert_eq!(parsed["error"]["data"]["code"], 503);
        assert_eq!(parsed["error"]["data"]["message"], "QUEUE");
    }
}
