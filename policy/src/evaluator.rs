use crate::parser::{
    AdvancedPolicyEngine, AdvancedRuleSpec, AgentPolicy, AgentSpec, FallbackMode, RuleAction,
    RuleSpec, TimeWindow,
};
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

#[derive(Debug, Clone, Copy)]
pub struct RuntimeEvaluationConfig {
    pub advanced_mode_enabled: bool,
}

impl RuntimeEvaluationConfig {
    pub fn from_env() -> Self {
        let advanced_mode_enabled = std::env::var("ASTRAGRAPH_POLICY_ADVANCED_MODE")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);
        Self {
            advanced_mode_enabled,
        }
    }
}

#[allow(dead_code)]
pub fn evaluate_policy(
    policy: &AgentPolicy,
    context: &PolicyContext<'_>,
) -> Result<EvaluationResult, EvaluationError> {
    evaluate_policy_with_runtime(
        policy,
        context,
        &RuntimeEvaluationConfig {
            advanced_mode_enabled: false,
        },
    )
}

pub fn evaluate_policy_with_runtime(
    policy: &AgentPolicy,
    context: &PolicyContext<'_>,
    runtime: &RuntimeEvaluationConfig,
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

    if runtime.advanced_mode_enabled {
        if let Some(advanced_mode) = policy.spec.runtime.advanced_mode {
            for rule in &policy.spec.advanced_rules {
                if !matches_advanced_expression(rule, advanced_mode.engine, context, agent.tier) {
                    continue;
                }
                return Ok(result_from_policy(
                    rule.action,
                    Some(rule.id.clone()),
                    &policy.spec.verification,
                    rule.require_confirmation.unwrap_or(false),
                ));
            }
        }
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

fn matches_advanced_expression(
    rule: &AdvancedRuleSpec,
    engine: AdvancedPolicyEngine,
    context: &PolicyContext<'_>,
    agent_tier: u32,
) -> bool {
    let expression = normalize_expression_for_engine(&rule.expression, engine);
    split_clauses(&expression)
        .iter()
        .all(|clause| matches_advanced_clause(clause, context, agent_tier))
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
    let clauses = split_clauses(&rule.condition);
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

fn normalize_expression_for_engine(expression: &str, engine: AdvancedPolicyEngine) -> String {
    match engine {
        AdvancedPolicyEngine::Dsl => expression.to_string(),
        AdvancedPolicyEngine::OpaCompat => expression
            .replace("input.action.tool", "action.tool")
            .replace("input.agent.tier", "agent_tier")
            .replace("input.action.args.", "action.args.")
            .replace("input.action.args", "action.args"),
    }
}

fn split_clauses(expression: &str) -> Vec<String> {
    expression
        .replace("&&", " AND ")
        .split("AND")
        .map(str::trim)
        .filter(|clause| !clause.is_empty())
        .map(|clause| clause.to_string())
        .collect()
}

fn matches_advanced_clause(clause: &str, context: &PolicyContext<'_>, agent_tier: u32) -> bool {
    if let Some((left, right)) = split_binary_clause(clause, " in ") {
        let Some(left_value) = resolve_expr_value(left, context, agent_tier) else {
            return false;
        };
        let list = right.trim().trim_start_matches('[').trim_end_matches(']');
        return list
            .split(',')
            .map(|item| trim_value(item).to_string())
            .any(|item| matches_text_value(&left_value, &item));
    }

    for operator in ["==", "!=", ">=", "<=", ">", "<"] {
        if let Some((left, right)) = split_binary_clause(clause, operator) {
            return compare_expression_values(left, operator, right, context, agent_tier);
        }
    }

    false
}

fn split_binary_clause<'a>(clause: &'a str, operator: &str) -> Option<(&'a str, &'a str)> {
    let (left, right) = clause.split_once(operator)?;
    Some((left.trim(), right.trim()))
}

#[derive(Debug, Clone)]
enum ExprValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

fn compare_expression_values(
    left: &str,
    operator: &str,
    right: &str,
    context: &PolicyContext<'_>,
    agent_tier: u32,
) -> bool {
    let Some(left_value) = resolve_expr_value(left, context, agent_tier) else {
        return false;
    };

    if let Some(number) = parse_expr_number(right) {
        return compare_numbers(left_value, operator, number);
    }
    if let Some(flag) = parse_expr_bool(right) {
        return compare_bools(left_value, operator, flag);
    }
    let text = trim_value(right).to_string();
    compare_text(left_value, operator, &text)
}

fn resolve_expr_value(
    expr: &str,
    context: &PolicyContext<'_>,
    agent_tier: u32,
) -> Option<ExprValue> {
    let normalized = expr.trim();
    if matches!(normalized, "action.tool" | "tool") {
        return Some(ExprValue::Text(context.tool_name.to_string()));
    }
    if matches!(normalized, "agent_tier" | "agent.tier") {
        return Some(ExprValue::Number(agent_tier as f64));
    }
    if let Some(arg_key) = normalized.strip_prefix("action.args.") {
        let value = context.args.get(arg_key)?;
        if let Some(number) = value.as_f64() {
            return Some(ExprValue::Number(number));
        }
        if let Some(flag) = value.as_bool() {
            return Some(ExprValue::Bool(flag));
        }
        if let Some(text) = value.as_str() {
            return Some(ExprValue::Text(text.to_string()));
        }
    }
    None
}

fn parse_expr_number(value: &str) -> Option<f64> {
    trim_value(value).parse::<f64>().ok()
}

fn parse_expr_bool(value: &str) -> Option<bool> {
    match trim_value(value).to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn compare_numbers(left_value: ExprValue, operator: &str, right_number: f64) -> bool {
    let ExprValue::Number(left_number) = left_value else {
        return false;
    };
    match operator {
        "==" => (left_number - right_number).abs() < 1e-9,
        "!=" => (left_number - right_number).abs() >= 1e-9,
        ">" => left_number > right_number,
        ">=" => left_number >= right_number,
        "<" => left_number < right_number,
        "<=" => left_number <= right_number,
        _ => false,
    }
}

fn compare_bools(left_value: ExprValue, operator: &str, right_flag: bool) -> bool {
    let ExprValue::Bool(left_flag) = left_value else {
        return false;
    };
    match operator {
        "==" => left_flag == right_flag,
        "!=" => left_flag != right_flag,
        _ => false,
    }
}

fn compare_text(left_value: ExprValue, operator: &str, right_text: &str) -> bool {
    match operator {
        "==" => matches_text_value(&left_value, right_text),
        "!=" => !matches_text_value(&left_value, right_text),
        _ => false,
    }
}

fn matches_text_value(left_value: &ExprValue, expected: &str) -> bool {
    match left_value {
        ExprValue::Text(text) => text == expected,
        ExprValue::Number(number) => format!("{number}") == expected,
        ExprValue::Bool(flag) => format!("{flag}") == expected,
    }
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
        parse_policy, AdvancedModeSpec, AdvancedPolicyEngine, AdvancedRuleSpec, AgentPolicy,
        AgentSpec, Metadata, PolicySpec, RuleSpec, RuntimeSpec, VerificationSpec,
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
                runtime: RuntimeSpec::default(),
                advanced_rules: Vec::new(),
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

    #[test]
    fn advanced_dsl_rule_applies_when_feature_flag_enabled() {
        let mut policy = base_policy();
        policy.spec.agents[0].allowed_tools.push("export_data".to_string());
        policy.spec.rules.clear();
        policy.spec.runtime = RuntimeSpec {
            version: "v2".to_string(),
            advanced_mode: Some(AdvancedModeSpec {
                engine: AdvancedPolicyEngine::Dsl,
            }),
        };
        policy.spec.advanced_rules = vec![AdvancedRuleSpec {
            id: "adv-block".to_string(),
            description: "Block export in advanced mode".to_string(),
            expression: "action.tool == export_data && agent_tier >= 3".to_string(),
            action: RuleAction::Block,
            require_confirmation: Some(true),
        }];

        let context = PolicyContext {
            agent_name: "agent",
            tool_name: "export_data",
            args: HashMap::new(),
            now_utc: None,
        };

        let disabled = evaluate_policy_with_runtime(
            &policy,
            &context,
            &RuntimeEvaluationConfig {
                advanced_mode_enabled: false,
            },
        )
        .expect("evaluation disabled");
        assert_eq!(disabled.decision, RuleAction::Allow);

        let enabled = evaluate_policy_with_runtime(
            &policy,
            &context,
            &RuntimeEvaluationConfig {
                advanced_mode_enabled: true,
            },
        )
        .expect("evaluation enabled");
        assert_eq!(enabled.decision, RuleAction::Block);
        assert_eq!(enabled.matched_rule_id.as_deref(), Some("adv-block"));
        assert!(enabled.require_confirmation);
    }

    #[test]
    fn advanced_opa_compat_expression_is_supported() {
        let mut policy = base_policy();
        policy.spec.agents[0].allowed_tools = vec!["review_summary".to_string()];
        policy.spec.rules.clear();
        policy.spec.runtime = RuntimeSpec {
            version: "v2".to_string(),
            advanced_mode: Some(AdvancedModeSpec {
                engine: AdvancedPolicyEngine::OpaCompat,
            }),
        };
        policy.spec.advanced_rules = vec![AdvancedRuleSpec {
            id: "adv-opa".to_string(),
            description: "Require approval for high amount".to_string(),
            expression: "input.action.tool == \"review_summary\" && input.action.args.amount >= 50000".to_string(),
            action: RuleAction::Block,
            require_confirmation: Some(false),
        }];

        let mut args = HashMap::new();
        args.insert("amount".to_string(), serde_yaml::Value::from(60000));
        let context = PolicyContext {
            agent_name: "agent",
            tool_name: "review_summary",
            args,
            now_utc: None,
        };

        let enabled = evaluate_policy_with_runtime(
            &policy,
            &context,
            &RuntimeEvaluationConfig {
                advanced_mode_enabled: true,
            },
        )
        .expect("evaluation");
        assert_eq!(enabled.decision, RuleAction::Block);
        assert_eq!(enabled.matched_rule_id.as_deref(), Some("adv-opa"));
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
