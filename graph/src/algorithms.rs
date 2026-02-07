use crate::ccg::{CausalGraph, EdgeType, GraphError, NodePayload, NodeType};
use petgraph::algo::toposort;
use petgraph::Direction;
use std::collections::HashSet;

#[allow(dead_code)]
pub fn validate_dag(graph: &CausalGraph) -> bool {
    toposort(graph.graph(), None).is_ok()
}

#[allow(dead_code)]
pub fn link_handoff(
    graph: &mut CausalGraph,
    handoff_node_id: &str,
    source_agent_id: &str,
) -> Result<(), GraphError> {
    let handoff_index = graph
        .node_index(handoff_node_id)
        .ok_or_else(|| GraphError::UnknownNode(handoff_node_id.to_string()))?;
    if let Some(action_index) = latest_action_node(graph, source_agent_id) {
        graph
            .graph_mut()
            .add_edge(action_index, handoff_index, EdgeType::InformedBy);
    }
    Ok(())
}

pub fn trace_drift_path(
    graph: &CausalGraph,
    start_node_id: &str,
    threshold: f32,
) -> Result<Vec<String>, GraphError> {
    let start_index = graph
        .node_index(start_node_id)
        .ok_or_else(|| GraphError::UnknownNode(start_node_id.to_string()))?;
    let mut visited = HashSet::new();
    let mut path = Vec::new();
    collect_earliest_path(graph, start_index, threshold, &mut visited, &mut path);
    Ok(path)
}

fn collect_earliest_path(
    graph: &CausalGraph,
    node_index: petgraph::graph::NodeIndex,
    threshold: f32,
    visited: &mut HashSet<petgraph::graph::NodeIndex>,
    path: &mut Vec<String>,
) {
    if !visited.insert(node_index) {
        return;
    }
    let mut parents: Vec<_> = graph
        .graph()
        .neighbors_directed(node_index, Direction::Incoming)
        .collect();
    parents.sort_by_key(|idx| idx.index());
    if let Some(parent) = parents.first().copied() {
        collect_earliest_path(graph, parent, threshold, visited, path);
    }
    if let Some(node) = graph.node(node_index) {
        path.push(node.id.clone());
        if let NodePayload::Verification {
            deviation_score, ..
        } = &node.payload
        {
            if *deviation_score < threshold {
                path.pop();
            }
        }
    }
}

#[allow(dead_code)]
fn latest_action_node(graph: &CausalGraph, agent_id: &str) -> Option<petgraph::graph::NodeIndex> {
    let mut latest: Option<petgraph::graph::NodeIndex> = None;
    for index in graph.graph().node_indices() {
        let node = graph.node(index)?;
        if node.node_type == NodeType::Action && node.agent_id == agent_id {
            latest = match latest {
                None => Some(index),
                Some(current) => {
                    if index.index() > current.index() {
                        Some(index)
                    } else {
                        Some(current)
                    }
                }
            };
        }
    }
    latest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ccg::{ActionStatus, GraphNode, NodePayload};

    #[test]
    fn validates_dag() {
        let mut graph = CausalGraph::new();
        let node1 = GraphNode {
            id: "n1".to_string(),
            node_type: NodeType::Thinking,
            agent_id: "agent".to_string(),
            workflow_id: "wf".to_string(),
            payload: NodePayload::Thinking {
                trace_id: "t1".to_string(),
                content: "reason".to_string(),
                model_name: "m".to_string(),
                token_count: 1,
            },
        };
        let node2 = GraphNode {
            id: "n2".to_string(),
            node_type: NodeType::Action,
            agent_id: "agent".to_string(),
            workflow_id: "wf".to_string(),
            payload: NodePayload::Action {
                action_type: "tool_call".to_string(),
                tool_name: "export_data".to_string(),
                arguments: serde_json::json!({}),
                status: ActionStatus::Blocked,
            },
        };
        let n1 = graph.upsert_node(node1);
        let n2 = graph.upsert_node(node2);
        graph.graph_mut().add_edge(n1, n2, EdgeType::CausedBy);
        assert!(validate_dag(&graph));
    }

    #[test]
    fn drift_path_returns_nodes() {
        let mut graph = CausalGraph::new();
        let node = GraphNode {
            id: "n1".to_string(),
            node_type: NodeType::Verification,
            agent_id: "agent".to_string(),
            workflow_id: "wf".to_string(),
            payload: NodePayload::Verification {
                parent_node_id: "p".to_string(),
                deviation_score: 0.9,
                policy_id: "policy".to_string(),
                verdict: ActionStatus::Blocked,
                verifier_model: "model".to_string(),
                latency_ms: 5,
            },
        };
        graph.upsert_node(node);
        let path = trace_drift_path(&graph, "n1", 0.7).expect("path");
        assert_eq!(path, vec!["n1".to_string()]);
    }
}
