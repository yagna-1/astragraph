use crate::evaluator::{evaluate_policy, PolicyContext};
use crate::parser::{FallbackMode as PolicyFallbackMode, RuleAction};
use crate::store::PolicyStore;
use astragraph_proto::astragraph::policy_service_server::PolicyService;
use astragraph_proto::astragraph::{PolicyEvaluationRequest, PolicyEvaluationResponse};
use prost_types::{value::Kind, Struct, Value};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tonic::{Request, Response, Status};
use tracing::info_span;

#[derive(Clone)]
pub struct PolicyServiceImpl {
    store: Arc<RwLock<PolicyStore>>,
}

impl PolicyServiceImpl {
    pub fn new(store: Arc<RwLock<PolicyStore>>) -> Self {
        Self { store }
    }
}

#[tonic::async_trait]
impl PolicyService for PolicyServiceImpl {
    async fn evaluate_action(
        &self,
        request: Request<PolicyEvaluationRequest>,
    ) -> Result<Response<PolicyEvaluationResponse>, Status> {
        let _span = info_span!("policy.evaluate_action").entered();
        let payload = request.into_inner();
        let policy_id = payload.policy_id.clone();
        let store = self.store.read().map_err(|_| Status::internal("store"))?;
        let policy_name = if policy_id.is_empty() {
            store
                .policies()
                .keys()
                .next()
                .cloned()
                .ok_or(Status::not_found("policy"))?
        } else {
            policy_id
        };
        let rollout_key = format!(
            "{}|{}|{:?}",
            payload.agent_id, payload.tool_name, payload.arguments
        );
        let policy = store
            .resolve_for_evaluation(&policy_name, &rollout_key)
            .ok_or(Status::not_found("policy"))?;

        let mut args = struct_to_yaml(payload.arguments);
        let now_utc = args
            .remove("__now_utc")
            .and_then(|value| value.as_str().map(str::to_string));
        let context = PolicyContext {
            agent_name: payload.agent_id.as_str(),
            tool_name: payload.tool_name.as_str(),
            args,
            now_utc: now_utc.as_deref(),
        };

        let result = evaluate_policy(policy, &context).map_err(|_| Status::internal("eval"))?;
        let response = PolicyEvaluationResponse {
            allowed: matches!(result.decision, RuleAction::Allow),
            rule_id: result.matched_rule_id.unwrap_or_default(),
            threshold: result.threshold,
            fallback: map_fallback(result.fallback) as i32,
            require_confirmation: result.require_confirmation,
        };

        Ok(Response::new(response))
    }
}

fn map_fallback(fallback: PolicyFallbackMode) -> astragraph_proto::astragraph::FallbackMode {
    match fallback {
        PolicyFallbackMode::Allow => astragraph_proto::astragraph::FallbackMode::FallbackAllow,
        PolicyFallbackMode::Block => astragraph_proto::astragraph::FallbackMode::FallbackBlock,
        PolicyFallbackMode::Queue => astragraph_proto::astragraph::FallbackMode::FallbackQueue,
    }
}

fn struct_to_yaml(struct_value: Option<Struct>) -> HashMap<String, serde_yaml::Value> {
    let mut map = HashMap::new();
    let struct_value = match struct_value {
        Some(value) => value,
        None => return map,
    };

    for (key, value) in struct_value.fields {
        map.insert(key, value_to_yaml(value));
    }

    map
}

fn value_to_yaml(value: Value) -> serde_yaml::Value {
    match value.kind {
        Some(Kind::NullValue(_)) => serde_yaml::Value::Null,
        Some(Kind::NumberValue(number)) => serde_yaml::Value::from(number),
        Some(Kind::StringValue(text)) => serde_yaml::Value::from(text),
        Some(Kind::BoolValue(flag)) => serde_yaml::Value::from(flag),
        Some(Kind::StructValue(struct_value)) => {
            let mapped = struct_value
                .fields
                .into_iter()
                .map(|(key, value)| (serde_yaml::Value::from(key), value_to_yaml(value)))
                .collect();
            serde_yaml::Value::Mapping(mapped)
        }
        Some(Kind::ListValue(list)) => {
            let items = list.values.into_iter().map(value_to_yaml).collect();
            serde_yaml::Value::Sequence(items)
        }
        None => serde_yaml::Value::Null,
    }
}
