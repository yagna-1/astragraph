use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackMode {
    Allow,
    Block,
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Block,
    Queue,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    #[allow(dead_code)]
    pub queue_timeout_ms: u64,
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            queue_timeout_ms: 500,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn decide(
        &self,
        policy_allowed: bool,
        has_thinking_trace: bool,
        allow_without_trace: bool,
        score: Option<f32>,
        threshold: f32,
        fallback: FallbackMode,
        time_window_ok: bool,
    ) -> Decision {
        if !policy_allowed || !time_window_ok {
            return Decision::Block;
        }

        if !has_thinking_trace && !allow_without_trace {
            return Decision::Block;
        }

        let score = match score {
            Some(score) => score,
            None => return fallback_to_decision(fallback),
        };

        if score < threshold {
            Decision::Allow
        } else {
            Decision::Block
        }
    }
}

fn fallback_to_decision(fallback: FallbackMode) -> Decision {
    match fallback {
        FallbackMode::Allow => Decision::Allow,
        FallbackMode::Block => Decision::Block,
        FallbackMode::Queue => Decision::Queue,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockedAction {
    pub tool: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockResponse {
    pub error: String,
    pub violation_id: String,
    pub rule_id: String,
    pub deviation_score: f32,
    pub threshold: f32,
    pub drift_path_origin: String,
    pub agent_id: String,
    pub blocked_action: BlockedAction,
    pub timestamp: String,
}
