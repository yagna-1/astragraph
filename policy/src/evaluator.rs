use crate::parser::{AgentPolicy, AgentSpec, FallbackMode, RuleAction, RuleSpec, TimeWindow};
use std::collections::HashMap;

#[derive(Debug)]
pub struct PolicyContext<'a> {
    pub agent_name: &'a str,
    pub tool_name: &'a str,
    pub args: HashMap<String, serde_yaml::Value>,
    pub now_utc: Option<&'a str>,
}

#[derive(Debug)]
pub struct EvaluationResult {
    pub decision: RuleAction,
    pub matched_rule_id: Option<String>,
    pub threshold: f32,
    pub fallback: FallbackMode,
    pub require_confirmation: bool,
}

#[derive(Debug)]
pub enum EvaluationError {
    AgentNotFound,
}

pub fn evaluate_policy(
    policy: &AgentPolicy,
    context: &PolicyContext<'_>,
) -> Result<EvaluationResult, EvaluationError> {
    let agent = policy
        .spec
        .agents
        .iter()
        .find(|agent| agent.name == context.agent_name)
        .ok_or(EvaluationError::AgentNotFound)?;

    if is_blocked_tool(agent, context.tool_name) {
        return Ok(result_from_policy(
            RuleAction::Block,
            None,
            &policy.spec.verification,
            false,
        ));
    }

    for rule in &policy.spec.rules {
        if !matches_condition(rule, context, agent.tier) {
            continue;
        }

        let decision = evaluate_time_window(rule, context);
        let require_confirmation = rule.require_confirmation.unwrap_or(false);

        return Ok(result_from_policy(
            decision,
            Some(rule.id.clone()),
            &policy.spec.verification,
            require_confirmation,
        ));
    }

    Ok(result_from_policy(
        RuleAction::Allow,
        None,
        &policy.spec.verification,
        false,
    ))
}

fn is_blocked_tool(agent: &AgentSpec, tool_name: &str) -> bool {
    if agent.blocked_tools.iter().any(|tool| tool == tool_name) {
        return true;
    }

    if !agent.allowed_tools.is_empty() && !agent.allowed_tools.iter().any(|tool| tool == tool_name)
    {
        return true;
    }

    false
}

fn evaluate_time_window(rule: &RuleSpec, context: &PolicyContext<'_>) -> RuleAction {
    let time_window = match &rule.time_window {
        Some(window) => window,
        None => return rule.action,
    };

    let now = match context.now_utc {
        Some(now) => now,
        None => return rule.action,
    };

    let within = is_within_time_window(now, time_window);
    if within {
        rule.action
    } else {
        rule.outside_window_action.unwrap_or(RuleAction::Block)
    }
}

fn matches_condition(rule: &RuleSpec, context: &PolicyContext<'_>, agent_tier: u32) -> bool {
    let clauses: Vec<&str> = rule.condition.split("AND").map(str::trim).collect();
    clauses
        .iter()
        .all(|clause| matches_clause(clause, context, agent_tier))
}

fn matches_clause(clause: &str, context: &PolicyContext<'_>, agent_tier: u32) -> bool {
    if let Some(expected) = clause.strip_prefix("action.tool ==") {
        let expected = trim_value(expected);
        return expected == context.tool_name;
    }

    if let Some(list) = clause.strip_prefix("action.tool in") {
        let list = list.trim();
        let list = list.trim_start_matches('[').trim_end_matches(']');
        let mut tools = list.split(',').map(trim_value);
        return tools.any(|tool| tool == context.tool_name);
    }

    if let Some(expr) = clause.strip_prefix("action.args.amount >") {
        let threshold = trim_value(expr).parse::<f64>().unwrap_or(f64::INFINITY);
        if let Some(value) = context.args.get("amount") {
            if let Some(amount) = value.as_f64() {
                return amount > threshold;
            }
        }
        return false;
    }

    if let Some(expr) = clause.strip_prefix("agent_tier >=") {
        let threshold = trim_value(expr).parse::<u32>().unwrap_or(u32::MAX);
        return agent_tier >= threshold;
    }

    false
}

fn trim_value(value: &str) -> &str {
    value.trim().trim_matches('"').trim_matches('\'')
}

fn is_within_time_window(now: &str, window: &TimeWindow) -> bool {
    let now_minutes = parse_time_to_minutes(now);
    let start_minutes = parse_time_to_minutes(&window.start);
    let end_minutes = parse_time_to_minutes(&window.end);

    match (now_minutes, start_minutes, end_minutes) {
        (Some(now), Some(start), Some(end)) if start <= end => now >= start && now <= end,
        (Some(now), Some(start), Some(end)) => now >= start || now <= end,
        _ => true,
    }
}

fn parse_time_to_minutes(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    if let Some((date, time)) = trimmed.split_once('T') {
        if date.contains('-')
            && time
                .chars()
                .next()
                .map(|ch| ch.is_ascii_digit())
                .unwrap_or(false)
        {
            return parse_hhmm(time.trim_end_matches('Z'));
        }
    }
    parse_hhmm(trimmed.trim_end_matches("UTC").trim())
}

fn parse_hhmm(value: &str) -> Option<u32> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let hours = parts[0].parse::<u32>().ok()?;
    let minutes = parts[1].parse::<u32>().ok()?;
    Some(hours.saturating_mul(60) + minutes)
}

fn result_from_policy(
    decision: RuleAction,
    matched_rule_id: Option<String>,
    verification: &crate::parser::VerificationSpec,
    require_confirmation: bool,
) -> EvaluationResult {
    EvaluationResult {
        decision,
        matched_rule_id,
        threshold: verification.threshold,
        fallback: verification.fallback,
        require_confirmation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{
        parse_policy, AgentPolicy, AgentSpec, Metadata, PolicySpec, RuleSpec, VerificationSpec,
    };
    use serde::Deserialize;
    use std::fs;
    use std::path::Path;

    fn base_policy() -> AgentPolicy {
        AgentPolicy {
            api_version: "astragraph.io/v1".to_string(),
            kind: "AgentPolicy".to_string(),
            metadata: Metadata {
                name: "test".to_string(),
                version: "1".to_string(),
                owner: "owner".to_string(),
                last_reviewed: None,
            },
            spec: PolicySpec {
                agents: vec![AgentSpec {
                    name: "agent".to_string(),
                    tier: 3,
                    allowed_tools: vec!["approve_invoice".to_string()],
                    blocked_tools: vec![],
                }],
                rules: vec![RuleSpec {
                    id: "rule-1".to_string(),
                    description: "No approvals over 50k".to_string(),
                    condition: "action.tool == approve_invoice AND action.args.amount > 50000"
                        .to_string(),
                    action: RuleAction::Block,
                    require_confirmation: Some(true),
                    time_window: None,
                    outside_window_action: None,
                }],
                verification: VerificationSpec {
                    threshold: 0.7,
                    model: "model".to_string(),
                    fallback: FallbackMode::Block,
                },
            },
        }
    }

    #[test]
    fn blocks_when_tool_not_allowed() {
        let policy = base_policy();
        let context = PolicyContext {
            agent_name: "agent",
            tool_name: "export_data",
            args: HashMap::new(),
            now_utc: None,
        };
        let result = evaluate_policy(&policy, &context).expect("evaluation");
        assert_eq!(result.decision, RuleAction::Block);
    }

    #[test]
    fn blocks_when_rule_matches() {
        let policy = base_policy();
        let mut args = HashMap::new();
        args.insert("amount".to_string(), serde_yaml::Value::from(60000));
        let context = PolicyContext {
            agent_name: "agent",
            tool_name: "approve_invoice",
            args,
            now_utc: None,
        };
        let result = evaluate_policy(&policy, &context).expect("evaluation");
        assert_eq!(result.decision, RuleAction::Block);
        assert_eq!(result.matched_rule_id.as_deref(), Some("rule-1"));
    }

    #[test]
    fn parses_utc_hhmm_without_iso_t_separator() {
        assert_eq!(parse_time_to_minutes("21:10 UTC"), Some(1270));
    }

    #[derive(Debug, Deserialize)]
    struct RegressionPack {
        name: String,
        policy: String,
        cases: Vec<RegressionCase>,
    }

    #[derive(Debug, Deserialize)]
    struct RegressionCase {
        name: String,
        agent: String,
        tool: String,
        #[serde(default)]
        args: HashMap<String, serde_yaml::Value>,
        #[serde(default)]
        now_utc: Option<String>,
        expect: RegressionExpectation,
    }

    #[derive(Debug, Deserialize)]
    struct RegressionExpectation {
        decision: RuleAction,
        #[serde(default)]
        rule_id: Option<String>,
        #[serde(default)]
        threshold: Option<f32>,
        #[serde(default)]
        fallback: Option<FallbackMode>,
        #[serde(default)]
        require_confirmation: Option<bool>,
    }

    fn regression_pack_paths() -> Vec<std::path::PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/policy_regressions");
        let mut paths = Vec::new();
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|value| value.to_str());
                if matches!(ext, Some("yaml" | "yml")) {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        paths
    }

    #[test]
    fn policy_regression_packs_pass() {
        let packs = regression_pack_paths();
        assert!(!packs.is_empty(), "no regression packs found");

        for pack_path in packs {
            let raw = fs::read_to_string(&pack_path)
                .unwrap_or_else(|err| panic!("failed reading {}: {err}", pack_path.display()));
            let pack: RegressionPack = serde_yaml::from_str(&raw)
                .unwrap_or_else(|err| panic!("failed parsing {}: {err}", pack_path.display()));
            let policy = parse_policy(&pack.policy).unwrap_or_else(|err| {
                panic!(
                    "pack '{}' has invalid policy in {}: {:?}",
                    pack.name,
                    pack_path.display(),
                    err
                )
            });

            for case in pack.cases {
                let context = PolicyContext {
                    agent_name: &case.agent,
                    tool_name: &case.tool,
                    args: case.args.clone(),
                    now_utc: case.now_utc.as_deref(),
                };
                let result = evaluate_policy(&policy, &context).unwrap_or_else(|_| {
                    panic!(
                        "case '{}' in pack '{}' failed evaluation",
                        case.name, pack.name
                    )
                });

                assert_eq!(
                    result.decision, case.expect.decision,
                    "pack '{}' case '{}' decision mismatch",
                    pack.name, case.name
                );
                if let Some(expected_rule_id) = case.expect.rule_id.as_deref() {
                    assert_eq!(
                        result.matched_rule_id.as_deref(),
                        Some(expected_rule_id),
                        "pack '{}' case '{}' rule mismatch",
                        pack.name,
                        case.name
                    );
                }
                if let Some(expected_threshold) = case.expect.threshold {
                    assert!(
                        (result.threshold - expected_threshold).abs() < 1e-6,
                        "pack '{}' case '{}' threshold mismatch: expected {}, got {}",
                        pack.name,
                        case.name,
                        expected_threshold,
                        result.threshold
                    );
                }
                if let Some(expected_fallback) = case.expect.fallback {
                    assert_eq!(
                        result.fallback, expected_fallback,
                        "pack '{}' case '{}' fallback mismatch",
                        pack.name, case.name
                    );
                }
                if let Some(expected_confirmation) = case.expect.require_confirmation {
                    assert_eq!(
                        result.require_confirmation, expected_confirmation,
                        "pack '{}' case '{}' confirmation mismatch",
                        pack.name, case.name
                    );
                }
            }
        }
    }
}
