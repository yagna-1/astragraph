use std::env;
use std::error::Error;
use std::fs;

#[path = "../parser.rs"]
mod parser;

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || has_flag(&args, "--help") {
        print_help();
        return Ok(());
    }

    let input = require_value(&args, "--input")?;
    let output = get_value(&args, "--output");
    let engine = parse_engine(get_value(&args, "--engine").as_deref())?;

    let raw = fs::read_to_string(&input)?;
    let mut policy = parser::parse_policy(&raw).map_err(|err| format!("{err:?}"))?;
    let advanced_rules = policy
        .spec
        .rules
        .iter()
        .map(|rule| parser::AdvancedRuleSpec {
            id: rule.id.clone(),
            description: format!("migrated: {}", rule.description),
            expression: migrate_expression(&rule.condition, engine),
            action: rule.action,
            require_confirmation: rule.require_confirmation,
        })
        .collect();

    policy.spec.runtime.version = "v2".to_string();
    policy.spec.runtime.advanced_mode = Some(parser::AdvancedModeSpec { engine });
    policy.spec.advanced_rules = advanced_rules;

    let rendered = serde_yaml::to_string(&policy)?;
    if let Some(output_path) = output {
        fs::write(output_path, rendered)?;
    } else {
        println!("{rendered}");
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "Usage:\n  \
cargo run -p astragraph-policy --bin policy_migrate -- \\\n  --input <policy.yaml> [--engine DSL|OPA_COMPAT] [--output <path>]\n"
    );
}

fn parse_engine(value: Option<&str>) -> Result<parser::AdvancedPolicyEngine, Box<dyn Error + Send + Sync>> {
    match value
        .unwrap_or("OPA_COMPAT")
        .trim()
        .to_ascii_uppercase()
        .as_str()
    {
        "DSL" => Ok(parser::AdvancedPolicyEngine::Dsl),
        "OPA_COMPAT" => Ok(parser::AdvancedPolicyEngine::OpaCompat),
        other => Err(format!("unsupported engine '{other}', use DSL or OPA_COMPAT").into()),
    }
}

fn migrate_expression(
    condition: &str,
    engine: parser::AdvancedPolicyEngine,
) -> String {
    let mut expression = condition.replace(" AND ", " && ").replace("AND", "&&");
    expression = quote_legacy_literals(&expression);
    match engine {
        parser::AdvancedPolicyEngine::Dsl => expression,
        parser::AdvancedPolicyEngine::OpaCompat => expression
            .replace("action.tool", "input.action.tool")
            .replace("action.args.", "input.action.args.")
            .replace("agent_tier", "input.agent.tier"),
    }
}

fn quote_legacy_literals(expression: &str) -> String {
    let mut clauses = Vec::new();
    for raw_clause in expression.split("&&") {
        let clause = raw_clause.trim();
        if let Some((left, right)) = clause.split_once("==") {
            let left = left.trim();
            let right = right.trim();
            if left == "action.tool" {
                clauses.push(format!("{left} == {}", maybe_quote_literal(right)));
                continue;
            }
        }
        if let Some((left, right)) = clause.split_once(" in ") {
            if left.trim() == "action.tool" {
                let normalized = right
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|item| maybe_quote_literal(item.trim()))
                    .collect::<Vec<_>>()
                    .join(", ");
                clauses.push(format!("{} in [{}]", left.trim(), normalized));
                continue;
            }
        }
        clauses.push(clause.to_string());
    }
    clauses.join(" && ")
}

fn maybe_quote_literal(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return trimmed.to_string();
    }
    if trimmed.eq_ignore_ascii_case("true")
        || trimmed.eq_ignore_ascii_case("false")
        || trimmed.parse::<f64>().is_ok()
    {
        return trimmed.to_string();
    }
    format!("\"{trimmed}\"")
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
