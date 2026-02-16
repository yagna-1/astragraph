use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs;

#[path = "../evaluator.rs"]
mod evaluator;
#[path = "../parser.rs"]
mod parser;

use evaluator::{PolicyContext, RuntimeEvaluationConfig};

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
    #[serde(default)]
    expect: Option<RegressionExpectation>,
}

#[derive(Debug, Deserialize)]
struct RegressionExpectation {
    decision: parser::RuleAction,
    #[serde(default)]
    rule_id: Option<String>,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || has_flag(&args, "--help") {
        print_help();
        return Ok(());
    }

    if let Some(pack_path) = get_value(&args, "--pack") {
        let strict = has_flag(&args, "--strict");
        let runtime = runtime_config(&args);
        return run_pack(&pack_path, strict, runtime);
    }

    let policy_path = require_value(&args, "--policy")?;
    let agent = require_value(&args, "--agent")?;
    let tool = require_value(&args, "--tool")?;
    let args_json = get_value(&args, "--args").unwrap_or_else(|| "{}".to_string());
    let now_utc = get_value(&args, "--now-utc");
    let runtime = runtime_config(&args);
    run_single(
        &policy_path,
        &agent,
        &tool,
        &args_json,
        now_utc.as_deref(),
        runtime,
    )
}

fn print_help() {
    eprintln!(
        "Usage:\n  \
cargo run -p astragraph-policy --bin policy_simulator -- \\\n  --policy <path> --agent <agent> --tool <tool> [--args <json>] [--now-utc <HH:MM UTC>] [--advanced-mode]\n\n  \
cargo run -p astragraph-policy --bin policy_simulator -- \\\n  --pack <tests/policy_regressions/*.yaml> [--strict] [--advanced-mode]\n"
    );
}

fn run_single(
    policy_path: &str,
    agent: &str,
    tool: &str,
    args_json: &str,
    now_utc: Option<&str>,
    runtime: RuntimeEvaluationConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let raw_policy = fs::read_to_string(policy_path)?;
    let policy = parser::parse_policy(&raw_policy).map_err(|err| format!("{:?}", err))?;
    let parsed_args = parse_args_json(args_json)?;

    let context = PolicyContext {
        agent_name: agent,
        tool_name: tool,
        args: parsed_args,
        now_utc,
    };
    let result = evaluator::evaluate_policy_with_runtime(&policy, &context, &runtime)
        .map_err(|_| "evaluation failed")?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "decision": format!("{:?}", result.decision).to_uppercase(),
            "rule_id": result.matched_rule_id,
            "threshold": result.threshold,
            "fallback": format!("{:?}", result.fallback).to_uppercase(),
            "require_confirmation": result.require_confirmation,
            "runtime_version": policy.spec.runtime.version,
            "advanced_mode_enabled": runtime.advanced_mode_enabled,
        }))?
    );
    Ok(())
}

fn run_pack(
    pack_path: &str,
    strict: bool,
    runtime: RuntimeEvaluationConfig,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let raw = fs::read_to_string(pack_path)?;
    let pack: RegressionPack = serde_yaml::from_str(&raw)?;
    let policy = parser::parse_policy(&pack.policy).map_err(|err| format!("{:?}", err))?;

    let mut failures = Vec::new();
    let mut results = Vec::new();

    for case in &pack.cases {
        let context = PolicyContext {
            agent_name: &case.agent,
            tool_name: &case.tool,
            args: case.args.clone(),
            now_utc: case.now_utc.as_deref(),
        };
        let eval = evaluator::evaluate_policy_with_runtime(&policy, &context, &runtime)
            .map_err(|_| "evaluation failed")?;
        let actual_decision = format!("{:?}", eval.decision).to_uppercase();
        let actual_rule = eval.matched_rule_id.clone();

        let mut matched = true;
        if let Some(expect) = &case.expect {
            let expected_decision = format!("{:?}", expect.decision).to_uppercase();
            if expected_decision != actual_decision {
                matched = false;
            }
            if let Some(expected_rule) = expect.rule_id.as_deref() {
                if actual_rule.as_deref() != Some(expected_rule) {
                    matched = false;
                }
            }
        }

        if strict && !matched {
            failures.push(case.name.clone());
        }

        results.push(json!({
            "case": case.name,
            "decision": actual_decision,
            "rule_id": actual_rule,
            "matches_expectation": matched
        }));
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "pack": pack.name,
            "cases": results,
            "strict": strict,
            "failures": failures,
            "advanced_mode_enabled": runtime.advanced_mode_enabled,
        }))?
    );

    if strict && !failures.is_empty() {
        return Err(format!("{} case(s) failed expectation", failures.len()).into());
    }

    Ok(())
}

fn runtime_config(args: &[String]) -> RuntimeEvaluationConfig {
    if has_flag(args, "--advanced-mode") {
        return RuntimeEvaluationConfig {
            advanced_mode_enabled: true,
        };
    }
    RuntimeEvaluationConfig::from_env()
}

fn parse_args_json(
    args_json: &str,
) -> Result<HashMap<String, serde_yaml::Value>, Box<dyn Error + Send + Sync>> {
    let parsed: serde_json::Value = serde_json::from_str(args_json)?;
    let mut out = HashMap::new();
    match parsed {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                out.insert(key, json_to_yaml(value));
            }
            Ok(out)
        }
        _ => Err("`--args` must be a JSON object".into()),
    }
}

fn json_to_yaml(value: serde_json::Value) -> serde_yaml::Value {
    match value {
        serde_json::Value::Null => serde_yaml::Value::Null,
        serde_json::Value::Bool(v) => serde_yaml::Value::Bool(v),
        serde_json::Value::Number(v) => serde_yaml::to_value(v).unwrap_or(serde_yaml::Value::Null),
        serde_json::Value::String(v) => serde_yaml::Value::String(v),
        serde_json::Value::Array(values) => {
            serde_yaml::Value::Sequence(values.into_iter().map(json_to_yaml).collect())
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_yaml::Mapping::new();
            for (key, value) in map {
                out.insert(serde_yaml::Value::String(key), json_to_yaml(value));
            }
            serde_yaml::Value::Mapping(out)
        }
    }
}

fn require_value(args: &[String], flag: &str) -> Result<String, Box<dyn Error + Send + Sync>> {
    get_value(args, flag).ok_or_else(|| format!("missing required flag: {flag}").into())
}

fn get_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1).cloned())
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}
