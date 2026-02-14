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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_when_policy_denies() {
        let circuit_breaker = CircuitBreaker::new();
        let decision = circuit_breaker.decide(
            false,
            true,
            false,
            Some(0.0),
            0.7,
            FallbackMode::Allow,
            true,
        );
        assert_eq!(decision, Decision::Block);
    }

    #[test]
    fn blocks_when_no_trace_and_not_allowlisted() {
        let circuit_breaker = CircuitBreaker::new();
        let decision = circuit_breaker.decide(
            true,
            false,
            false,
            Some(0.01),
            0.7,
            FallbackMode::Allow,
            true,
        );
        assert_eq!(decision, Decision::Block);
    }

    #[test]
    fn queues_when_verifier_missing_and_fallback_queue() {
        let circuit_breaker = CircuitBreaker::new();
        let decision = circuit_breaker.decide(
            true,
            true,
            false,
            None,
            0.7,
            FallbackMode::Queue,
            true,
        );
        assert_eq!(decision, Decision::Queue);
    }

    #[test]
    fn allows_when_score_below_threshold() {
        let circuit_breaker = CircuitBreaker::new();
        let decision = circuit_breaker.decide(
            true,
            true,
            false,
            Some(0.2),
            0.7,
            FallbackMode::Block,
            true,
        );
        assert_eq!(decision, Decision::Allow);
    }

    #[test]
    fn blocks_when_score_meets_or_exceeds_threshold() {
        let circuit_breaker = CircuitBreaker::new();
        let at_threshold = circuit_breaker.decide(
            true,
            true,
            false,
            Some(0.7),
            0.7,
            FallbackMode::Allow,
            true,
        );
        let above_threshold = circuit_breaker.decide(
            true,
            true,
            false,
            Some(0.9),
            0.7,
            FallbackMode::Allow,
            true,
        );
        assert_eq!(at_threshold, Decision::Block);
        assert_eq!(above_threshold, Decision::Block);
    }
}
