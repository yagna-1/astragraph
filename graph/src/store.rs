use crate::algorithms;
use crate::ccg::{
    ActionStatus, CausalGraph, EdgeType, GraphError, GraphNode, NodePayload, NodeType,
};
use crate::pii_scrubber::PiiScrubber;
use astragraph_proto::astragraph::{Edge, GraphNode as ProtoNode, NodeType as ProtoNodeType};
use prost_types::{value::Kind, ListValue, Struct, Value};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct GraphStore {
    graphs: HashMap<String, GraphData>,
    flush_dir: PathBuf,
    ttl: Duration,
    scrubber: PiiScrubber,
    violations: Vec<ViolationRecord>,
}

struct GraphData {
    ccg: CausalGraph,
    workflow_id: String,
    status: String,
    last_updated: Instant,
}

#[derive(Debug, Serialize, Clone)]
pub struct ViolationRecord {
    pub violation_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub rule_id: String,
    pub agent_id: String,
    pub timestamp: i64,
    pub deviation_score: f32,
    pub threshold: f32,
    pub snapshot: serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct GraphResponse {
    pub graph_id: String,
    pub workflow_id: String,
    pub nodes: Vec<GraphNodeResponse>,
    pub edges: Vec<GraphEdgeResponse>,
}

#[derive(Debug, Serialize, Clone)]
pub struct GraphNodeResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deviation_score: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct GraphEdgeResponse {
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub edge_type: String,
}

#[derive(Debug, Serialize)]
pub struct GraphSummary {
    pub graph_id: String,
    pub workflow_id: String,
    pub status: String,
}

#[derive(Debug, Default)]
pub struct NodeFilter {
    pub node_type: Option<String>,
    pub agent_id: Option<String>,
    pub status: Option<String>,
}

impl GraphStore {
    pub fn new(flush_dir: PathBuf, ttl: Duration) -> Self {
        Self {
            graphs: HashMap::new(),
            flush_dir,
            ttl,
            scrubber: PiiScrubber::new(),
            violations: Vec::new(),
        }
    }

    pub fn insert_node(&mut self, proto_node: ProtoNode) -> Result<String, GraphError> {
        let graph_id = proto_node.workflow_id.clone();
        let graph = self
            .graphs
            .entry(graph_id.clone())
            .or_insert_with(|| GraphData {
                ccg: CausalGraph::new(),
                workflow_id: proto_node.workflow_id.clone(),
                status: "active".to_string(),
                last_updated: Instant::now(),
            });

        let inserted_node_id = {
            let previous = graph.ccg.clone();
            let mut node = convert_node(proto_node)?;
            scrub_node_payload(&self.scrubber, &mut node);
            let inserted_node_id = node.id.clone();
            let inserted_agent_id = node.agent_id.clone();
            let is_handoff = node.node_type == NodeType::Handoff;
            graph.ccg.upsert_node(node);
            if is_handoff {
                let _ =
                    algorithms::link_handoff(&mut graph.ccg, &inserted_node_id, &inserted_agent_id);
            }
            if !algorithms::validate_dag(&graph.ccg) {
                graph.ccg = previous;
                return Err(GraphError::UnknownNode("cycle detected".to_string()));
            }
            graph.last_updated = Instant::now();
            inserted_node_id
        };
        self.capture_violation_if_needed(&graph_id, &inserted_node_id);
        Ok(graph_id)
    }

    pub fn link_edge(&mut self, edge: Edge) -> Result<(), GraphError> {
        let graph_id = self
            .graphs
            .iter()
            .find_map(|(graph_id, data)| {
                if data.ccg.node_index(&edge.from).is_some()
                    || data.ccg.node_index(&edge.to).is_some()
                {
                    Some(graph_id.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| GraphError::UnknownNode("unknown graph".to_string()))?;
        let graph = self
            .graphs
            .get_mut(&graph_id)
            .ok_or_else(|| GraphError::UnknownNode(graph_id.clone()))?;
        let previous = graph.ccg.clone();
        let edge_type = convert_edge_type(edge.r#type)?;
        graph.ccg.add_edge_by_id(&edge.from, &edge.to, edge_type)?;
        if !algorithms::validate_dag(&graph.ccg) {
            graph.ccg = previous;
            return Err(GraphError::UnknownNode("cycle detected".to_string()));
        }
        graph.last_updated = Instant::now();
        Ok(())
    }

    pub fn get_graph(&self, graph_id: &str) -> Option<GraphResponse> {
        let graph = self.graphs.get(graph_id)?;
        Some(build_graph_response(graph_id, graph))
    }

    pub fn list_graphs(&self, status: Option<&str>) -> Vec<GraphSummary> {
        self.graphs
            .iter()
            .filter(|(_, data)| status.is_none_or(|value| data.status == value))
            .map(|(graph_id, data)| GraphSummary {
                graph_id: graph_id.clone(),
                workflow_id: data.workflow_id.clone(),
                status: data.status.clone(),
            })
            .collect()
    }

    pub fn list_nodes(
        &self,
        graph_id: &str,
        filter: &NodeFilter,
    ) -> Option<Vec<GraphNodeResponse>> {
        let graph = self.graphs.get(graph_id)?;
        let response = build_graph_response(graph_id, graph);
        let nodes = response
            .nodes
            .into_iter()
            .filter(|node| {
                filter
                    .node_type
                    .as_ref()
                    .is_none_or(|value| node.node_type == *value)
            })
            .filter(|node| {
                filter
                    .agent_id
                    .as_ref()
                    .is_none_or(|value| node.agent_id.as_ref() == Some(value))
            })
            .filter(|node| {
                filter
                    .status
                    .as_ref()
                    .is_none_or(|value| node.status.as_ref() == Some(value))
            })
            .collect();
        Some(nodes)
    }

    pub fn drift_path(&self, graph_id: &str, node_id: &str, threshold: f32) -> Option<Vec<String>> {
        let graph = self.graphs.get(graph_id)?;
        algorithms::trace_drift_path(&graph.ccg, node_id, threshold).ok()
    }

    pub fn flush_to_disk(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.flush_dir)?;
        for (graph_id, graph) in &self.graphs {
            let response = build_graph_response(graph_id, graph);
            let path = self.flush_dir.join(format!("{graph_id}.jsonl"));
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            let line = serde_json::to_string(&response).unwrap_or_default();
            writeln!(file, "{line}")?;
        }
        let violations_path = self.flush_dir.join("violations.jsonl");
        let mut violations = OpenOptions::new()
            .create(true)
            .append(true)
            .open(violations_path)?;
        for record in &self.violations {
            let line = serde_json::to_string(record).unwrap_or_default();
            writeln!(violations, "{line}")?;
        }
        Ok(())
    }

    pub fn cleanup_expired(&mut self) {
        let ttl = self.ttl;
        self.graphs
            .retain(|_, data| data.last_updated.elapsed() <= ttl);
    }

    pub fn violations(&self) -> Vec<ViolationRecord> {
        self.violations.clone()
    }

    pub fn violation(&self, violation_id: &str) -> Option<ViolationRecord> {
        self.violations
            .iter()
            .find(|record| record.violation_id == violation_id)
            .cloned()
    }

    pub fn violations_csv(&self) -> String {
        let mut csv =
            "violation_id,workflow_id,node_id,rule_id,agent_id,timestamp,deviation_score,threshold\n"
                .to_string();
        for record in &self.violations {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                record.violation_id,
                record.workflow_id,
                record.node_id,
                record.rule_id,
                record.agent_id,
                record.timestamp,
                record.deviation_score,
                record.threshold
            ));
        }
        csv
    }

    fn capture_violation_if_needed(&mut self, graph_id: &str, node_id: &str) {
        let Some(graph) = self.graphs.get(graph_id) else {
            return;
        };
        let Some(index) = graph.ccg.node_index(node_id) else {
            return;
        };
        let Some(node) = graph.ccg.node(index) else {
            return;
        };
        let (rule_id, deviation_score, threshold) = match &node.payload {
            NodePayload::Verification {
                deviation_score,
                policy_id,
                verdict,
                ..
            } => {
                if !matches!(verdict, ActionStatus::Blocked) {
                    return;
                }
                (policy_id.clone(), *deviation_score, 0.7)
            }
            NodePayload::Action {
                arguments, status, ..
            } => {
                if !matches!(status, ActionStatus::Blocked) {
                    return;
                }
                let rule_id = arguments
                    .get("__policy_rule_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string();
                if rule_id.is_empty() {
                    return;
                }
                (rule_id, 1.0, 1.0)
            }
            _ => return,
        };
        let snapshot = json!(build_graph_response(graph_id, graph));
        self.violations.push(ViolationRecord {
            violation_id: format!("violation-{}", node.id),
            workflow_id: graph_id.to_string(),
            node_id: node.id.clone(),
            rule_id,
            agent_id: node.agent_id.clone(),
            timestamp: now_unix_ts(),
            deviation_score,
            threshold,
            snapshot,
        });
    }
}

fn now_unix_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

fn build_graph_response(graph_id: &str, graph: &GraphData) -> GraphResponse {
    let nodes = graph
        .ccg
        .graph()
        .node_indices()
        .filter_map(|index| graph.ccg.node(index))
        .map(graph_node_response)
        .collect();

    let edges = graph
        .ccg
        .graph()
        .edge_indices()
        .filter_map(|edge_index| {
            let (from, to) = graph.ccg.graph().edge_endpoints(edge_index)?;
            let edge_type = graph.ccg.graph().edge_weight(edge_index)?;
            Some(GraphEdgeResponse {
                from: graph.ccg.node(from)?.id.clone(),
                to: graph.ccg.node(to)?.id.clone(),
                edge_type: format!("{edge_type:?}"),
            })
        })
        .collect();

    GraphResponse {
        graph_id: graph_id.to_string(),
        workflow_id: graph.workflow_id.clone(),
        nodes,
        edges,
    }
}

fn graph_node_response(node: &GraphNode) -> GraphNodeResponse {
    match &node.payload {
        NodePayload::Thinking { .. } => GraphNodeResponse {
            id: node.id.clone(),
            node_type: "ThinkingNode".to_string(),
            agent_id: Some(node.agent_id.clone()),
            action_type: None,
            tool_name: None,
            deviation_score: None,
            status: None,
        },
        NodePayload::Action {
            action_type,
            tool_name,
            status,
            ..
        } => GraphNodeResponse {
            id: node.id.clone(),
            node_type: "ActionNode".to_string(),
            agent_id: Some(node.agent_id.clone()),
            action_type: Some(action_type.clone()),
            tool_name: Some(tool_name.clone()),
            deviation_score: None,
            status: Some(format!("{status:?}").to_lowercase()),
        },
        NodePayload::Handoff { .. } => GraphNodeResponse {
            id: node.id.clone(),
            node_type: "HandoffNode".to_string(),
            agent_id: Some(node.agent_id.clone()),
            action_type: None,
            tool_name: None,
            deviation_score: None,
            status: None,
        },
        NodePayload::Verification {
            deviation_score, ..
        } => GraphNodeResponse {
            id: node.id.clone(),
            node_type: "VerificationNode".to_string(),
            agent_id: Some(node.agent_id.clone()),
            action_type: None,
            tool_name: None,
            deviation_score: Some(*deviation_score),
            status: None,
        },
    }
}

fn convert_node(proto_node: ProtoNode) -> Result<GraphNode, GraphError> {
    let ProtoNode {
        id,
        r#type,
        agent_id,
        workflow_id,
        payload,
        ..
    } = proto_node;

    let payload = match payload {
        Some(astragraph_proto::astragraph::graph_node::Payload::Thinking(thinking)) => {
            NodePayload::Thinking {
                trace_id: thinking.trace_id,
                content: thinking.content,
                model_name: thinking.model_name,
                token_count: thinking.token_count,
            }
        }
        Some(astragraph_proto::astragraph::graph_node::Payload::Action(action)) => {
            let status = convert_action_status(action.status());
            NodePayload::Action {
                action_type: action.action_type,
                tool_name: action.tool_name,
                arguments: struct_to_json(action.arguments),
                status,
            }
        }
        Some(astragraph_proto::astragraph::graph_node::Payload::Handoff(handoff)) => {
            NodePayload::Handoff {
                source_agent_id: handoff.source_agent_id,
                target_agent_id: handoff.target_agent_id,
                task_id: handoff.task_id,
                context_hash: handoff.context_hash,
            }
        }
        Some(astragraph_proto::astragraph::graph_node::Payload::Verification(verification)) => {
            let verdict = convert_action_status(verification.verdict());
            NodePayload::Verification {
                parent_node_id: verification.parent_node_id,
                deviation_score: verification.deviation_score,
                policy_id: verification.policy_id,
                verdict,
                verifier_model: verification.verifier_model,
                latency_ms: verification.latency_ms,
            }
        }
        None => return Err(GraphError::UnknownNode("missing payload".to_string())),
    };

    let node_type = convert_node_type(r#type)?;

    Ok(GraphNode {
        id,
        node_type,
        agent_id,
        workflow_id,
        payload,
    })
}

fn scrub_node_payload(scrubber: &PiiScrubber, node: &mut GraphNode) {
    if let NodePayload::Thinking { content, .. } = &mut node.payload {
        *content = scrubber.scrub(content);
    }
}

fn convert_node_type(node_type: i32) -> Result<NodeType, GraphError> {
    match ProtoNodeType::try_from(node_type) {
        Ok(ProtoNodeType::Thinking) => Ok(NodeType::Thinking),
        Ok(ProtoNodeType::Action) => Ok(NodeType::Action),
        Ok(ProtoNodeType::Handoff) => Ok(NodeType::Handoff),
        Ok(ProtoNodeType::Verification) => Ok(NodeType::Verification),
        Err(_) => Err(GraphError::UnknownNode("invalid node type".to_string())),
    }
}

fn convert_edge_type(edge_type: i32) -> Result<EdgeType, GraphError> {
    match astragraph_proto::astragraph::EdgeType::try_from(edge_type) {
        Ok(astragraph_proto::astragraph::EdgeType::CausedBy) => Ok(EdgeType::CausedBy),
        Ok(astragraph_proto::astragraph::EdgeType::InformedBy) => Ok(EdgeType::InformedBy),
        Ok(astragraph_proto::astragraph::EdgeType::VerifiedBy) => Ok(EdgeType::VerifiedBy),
        Err(_) => Err(GraphError::UnknownNode("invalid edge type".to_string())),
    }
}

fn convert_action_status(status: astragraph_proto::astragraph::ActionStatus) -> ActionStatus {
    match status {
        astragraph_proto::astragraph::ActionStatus::Pending => ActionStatus::Pending,
        astragraph_proto::astragraph::ActionStatus::Allowed => ActionStatus::Allowed,
        astragraph_proto::astragraph::ActionStatus::Blocked => ActionStatus::Blocked,
    }
}

fn struct_to_json(struct_value: Option<Struct>) -> serde_json::Value {
    let Some(struct_value) = struct_value else {
        return json!({});
    };
    let mut map = serde_json::Map::new();
    for (key, value) in struct_value.fields {
        map.insert(key, prost_value_to_json(value));
    }
    serde_json::Value::Object(map)
}

fn prost_value_to_json(value: Value) -> serde_json::Value {
    match value.kind {
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::NumberValue(number)) => json!(number),
        Some(Kind::StringValue(text)) => json!(text),
        Some(Kind::BoolValue(flag)) => json!(flag),
        Some(Kind::StructValue(struct_value)) => struct_to_json(Some(struct_value)),
        Some(Kind::ListValue(ListValue { values })) => {
            serde_json::Value::Array(values.into_iter().map(prost_value_to_json).collect())
        }
        None => serde_json::Value::Null,
    }
}
