use petgraph::graph::{Graph, NodeIndex};
use petgraph::Directed;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeType {
    Thinking,
    Action,
    Handoff,
    Verification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionStatus {
    Pending,
    Allowed,
    Blocked,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EdgeType {
    CausedBy,
    InformedBy,
    VerifiedBy,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum NodePayload {
    Thinking {
        trace_id: String,
        content: String,
        model_name: String,
        token_count: u32,
    },
    Action {
        action_type: String,
        tool_name: String,
        arguments: serde_json::Value,
        status: ActionStatus,
    },
    Handoff {
        source_agent_id: String,
        target_agent_id: String,
        task_id: String,
        context_hash: String,
    },
    Verification {
        parent_node_id: String,
        deviation_score: f32,
        policy_id: String,
        verdict: ActionStatus,
        verifier_model: String,
        latency_ms: u32,
    },
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub node_type: NodeType,
    pub agent_id: String,
    pub workflow_id: String,
    pub payload: NodePayload,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum GraphError {
    UnknownNode(String),
}

#[derive(Clone)]
pub struct CausalGraph {
    graph: Graph<GraphNode, EdgeType, Directed>,
    node_index: HashMap<String, NodeIndex>,
}

impl CausalGraph {
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
            node_index: HashMap::new(),
        }
    }

    pub fn upsert_node(&mut self, node: GraphNode) -> NodeIndex {
        if let Some(index) = self.node_index.get(&node.id) {
            return *index;
        }
        let index = self.graph.add_node(node.clone());
        self.node_index.insert(node.id, index);
        index
    }

    pub fn add_edge_by_id(
        &mut self,
        from_id: &str,
        to_id: &str,
        edge_type: EdgeType,
    ) -> Result<(), GraphError> {
        let from = self
            .node_index
            .get(from_id)
            .copied()
            .ok_or_else(|| GraphError::UnknownNode(from_id.to_string()))?;
        let to = self
            .node_index
            .get(to_id)
            .copied()
            .ok_or_else(|| GraphError::UnknownNode(to_id.to_string()))?;
        self.graph.add_edge(from, to, edge_type);
        Ok(())
    }

    pub fn node_index(&self, node_id: &str) -> Option<NodeIndex> {
        self.node_index.get(node_id).copied()
    }

    pub fn node(&self, index: NodeIndex) -> Option<&GraphNode> {
        self.graph.node_weight(index)
    }

    pub fn graph(&self) -> &Graph<GraphNode, EdgeType, Directed> {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut Graph<GraphNode, EdgeType, Directed> {
        &mut self.graph
    }
}
