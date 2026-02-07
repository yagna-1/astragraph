use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentPolicy {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: PolicySpec,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Metadata {
    pub name: String,
    pub version: String,
    pub owner: String,
    #[serde(default, rename = "last_reviewed")]
    pub last_reviewed: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PolicySpec {
    pub agents: Vec<AgentSpec>,
    pub rules: Vec<RuleSpec>,
    pub verification: VerificationSpec,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentSpec {
    pub name: String,
    pub tier: u32,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub blocked_tools: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RuleSpec {
    pub id: String,
    pub description: String,
    pub condition: String,
    pub action: RuleAction,
    #[serde(default)]
    pub require_confirmation: Option<bool>,
    #[serde(default)]
    pub time_window: Option<TimeWindow>,
    #[serde(default)]
    pub outside_window_action: Option<RuleAction>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TimeWindow {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum RuleAction {
    Allow,
    Block,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct VerificationSpec {
    pub threshold: f32,
    pub model: String,
    pub fallback: FallbackMode,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum FallbackMode {
    Allow,
    Block,
    Queue,
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum PolicyError {
    Parse(serde_yaml::Error),
    InvalidFormat(&'static str),
}

impl From<serde_yaml::Error> for PolicyError {
    fn from(err: serde_yaml::Error) -> Self {
        PolicyError::Parse(err)
    }
}

pub fn parse_policy(raw_yaml: &str) -> Result<AgentPolicy, PolicyError> {
    let policy: AgentPolicy = serde_yaml::from_str(raw_yaml)?;
    if policy.api_version != "astragraph.io/v1" {
        return Err(PolicyError::InvalidFormat("unsupported apiVersion"));
    }
    if policy.kind != "AgentPolicy" {
        return Err(PolicyError::InvalidFormat("unsupported kind"));
    }
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_policy() {
        let yaml = r#"
apiVersion: astragraph.io/v1
kind: AgentPolicy
metadata:
  name: finance-approval-guardrails
  version: "2.1"
  owner: "security-team@company.com"
spec:
  agents:
    - name: finance-approver
      tier: 3
      allowed_tools: [approve_invoice]
  rules: []
  verification:
    threshold: 0.7
    model: "lfm2.5-1.2b-distilled-v2"
    fallback: BLOCK
"#;
        let policy = parse_policy(yaml).expect("parse");
        assert_eq!(policy.metadata.name, "finance-approval-guardrails");
    }

    #[test]
    fn rejects_invalid_version() {
        let yaml = r#"
apiVersion: wrong/v1
kind: AgentPolicy
metadata:
  name: test
  version: "1"
  owner: "owner"
spec:
  agents: []
  rules: []
  verification:
    threshold: 0.5
    model: "model"
    fallback: BLOCK
"#;
        let err = parse_policy(yaml).expect_err("error");
        match err {
            PolicyError::InvalidFormat(_) => {}
            _ => panic!("unexpected error"),
        }
    }
}
