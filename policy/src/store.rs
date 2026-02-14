use crate::parser::AgentPolicy;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[derive(Default)]
pub struct PolicyStore {
    policies: HashMap<String, AgentPolicy>,
    rollouts: HashMap<String, ActiveRollout>,
    history: HashMap<String, Vec<PolicyHistoryItem>>,
    history_path: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct PolicySummary {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PolicyHistoryItem {
    pub version: String,
    pub timestamp: String,
    pub diff: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PolicyRolloutState {
    pub name: String,
    pub stable_version: String,
    pub candidate_version: String,
    pub percentage: u8,
    pub started_at: String,
}

#[derive(Debug)]
pub enum StoreError {
    NotFound,
    InvalidInput(&'static str),
}

struct ActiveRollout {
    candidate: AgentPolicy,
    percentage: u8,
    started_at: String,
    stable_version: String,
}

impl PolicyStore {
    pub fn new() -> Self {
        Self {
            policies: HashMap::new(),
            rollouts: HashMap::new(),
            history: HashMap::new(),
            history_path: PathBuf::from("data/policies/history.jsonl"),
        }
    }

    pub fn insert(&mut self, policy: AgentPolicy) {
        let name = policy.metadata.name.clone();
        let next_version = policy.metadata.version.clone();
        let diff = self
            .policies
            .get(&name)
            .map(|previous| {
                let before = serde_yaml::to_string(previous).unwrap_or_default();
                let after = serde_yaml::to_string(&policy).unwrap_or_default();
                if before == after {
                    "no-change".to_string()
                } else {
                    format!("len:{}->{}", before.len(), after.len())
                }
            })
            .unwrap_or_else(|| "initial".to_string());
        self.policies.insert(name.clone(), policy);
        let item = PolicyHistoryItem {
            version: next_version,
            timestamp: format!(
                "{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|v| v.as_secs())
                    .unwrap_or(0)
            ),
            diff,
        };
        self.history.entry(name).or_default().push(item.clone());
        let _ = self.persist_history_item(&item);
    }

    pub fn start_rollout(
        &mut self,
        policy_name: &str,
        candidate: AgentPolicy,
        percentage: u8,
    ) -> Result<PolicyRolloutState, StoreError> {
        if percentage == 0 || percentage > 100 {
            return Err(StoreError::InvalidInput(
                "rollout percentage must be in range 1..=100",
            ));
        }
        if candidate.metadata.name != policy_name {
            return Err(StoreError::InvalidInput(
                "candidate policy metadata.name must match target policy name",
            ));
        }
        let stable = self.policies.get(policy_name).ok_or(StoreError::NotFound)?;
        let rollout = ActiveRollout {
            candidate: candidate.clone(),
            percentage,
            started_at: unix_now().to_string(),
            stable_version: stable.metadata.version.clone(),
        };
        self.rollouts.insert(policy_name.to_string(), rollout);
        Ok(self
            .rollouts
            .get(policy_name)
            .map(|value| PolicyRolloutState {
                name: policy_name.to_string(),
                stable_version: value.stable_version.clone(),
                candidate_version: value.candidate.metadata.version.clone(),
                percentage: value.percentage,
                started_at: value.started_at.clone(),
            })
            .expect("rollout inserted"))
    }

    pub fn rollout(&self, policy_name: &str) -> Option<PolicyRolloutState> {
        self.rollouts.get(policy_name).map(|value| PolicyRolloutState {
            name: policy_name.to_string(),
            stable_version: value.stable_version.clone(),
            candidate_version: value.candidate.metadata.version.clone(),
            percentage: value.percentage,
            started_at: value.started_at.clone(),
        })
    }

    pub fn stop_rollout(&mut self, policy_name: &str) -> bool {
        self.rollouts.remove(policy_name).is_some()
    }

    pub fn promote_rollout(&mut self, policy_name: &str) -> Result<PolicySummary, StoreError> {
        let rollout = self.rollouts.remove(policy_name).ok_or(StoreError::NotFound)?;
        self.insert(rollout.candidate);
        let policy = self.policies.get(policy_name).ok_or(StoreError::NotFound)?;
        Ok(PolicySummary {
            name: policy.metadata.name.clone(),
            version: policy.metadata.version.clone(),
        })
    }

    pub fn resolve_for_evaluation<'a>(
        &'a self,
        policy_name: &str,
        bucketing_key: &str,
    ) -> Option<&'a AgentPolicy> {
        let stable = self.policies.get(policy_name)?;
        if let Some(rollout) = self.rollouts.get(policy_name) {
            if bucket_0_to_99(bucketing_key) < rollout.percentage {
                return Some(&rollout.candidate);
            }
        }
        Some(stable)
    }

    pub fn get(&self, name: &str) -> Option<&AgentPolicy> {
        self.policies.get(name)
    }

    pub fn list(&self) -> Vec<PolicySummary> {
        self.policies
            .values()
            .map(|policy| PolicySummary {
                name: policy.metadata.name.clone(),
                version: policy.metadata.version.clone(),
            })
            .collect()
    }

    pub fn policies(&self) -> &HashMap<String, AgentPolicy> {
        &self.policies
    }

    pub fn history(&self, name: &str) -> Vec<PolicyHistoryItem> {
        self.history.get(name).cloned().unwrap_or_default()
    }

    fn persist_history_item(&self, item: &PolicyHistoryItem) -> std::io::Result<()> {
        if let Some(parent) = self.history_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.history_path)?;
        let line = serde_json::to_string(item).unwrap_or_default();
        writeln!(file, "{line}")?;
        Ok(())
    }
}

fn bucket_0_to_99(seed: &str) -> u8 {
    let mut hash: u64 = 1469598103934665603;
    for byte in seed.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    (hash % 100) as u8
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_policy;

    fn base_policy(version: &str) -> AgentPolicy {
        let raw = format!(
            r#"
apiVersion: astragraph.io/v1
kind: AgentPolicy
metadata:
  name: rollout-test
  version: "{version}"
  owner: "owner"
spec:
  agents:
    - name: agent
      tier: 3
      allowed_tools: [safe_tool]
      blocked_tools: []
  rules: []
  verification:
    threshold: 0.7
    model: "model"
    fallback: BLOCK
"#
        );
        parse_policy(&raw).expect("policy parse")
    }

    #[test]
    fn can_start_and_stop_rollout() {
        let mut store = PolicyStore::new();
        store.insert(base_policy("1"));
        let state = store
            .start_rollout("rollout-test", base_policy("2"), 30)
            .expect("rollout start");
        assert_eq!(state.candidate_version, "2");
        assert!(store.rollout("rollout-test").is_some());
        assert!(store.stop_rollout("rollout-test"));
        assert!(store.rollout("rollout-test").is_none());
    }

    #[test]
    fn promote_rollout_replaces_stable_version() {
        let mut store = PolicyStore::new();
        store.insert(base_policy("1"));
        store
            .start_rollout("rollout-test", base_policy("2"), 100)
            .expect("rollout start");
        let promoted = store.promote_rollout("rollout-test").expect("promote");
        assert_eq!(promoted.version, "2");
        assert!(store.rollout("rollout-test").is_none());
        assert_eq!(
            store.get("rollout-test").map(|value| value.metadata.version.as_str()),
            Some("2")
        );
    }
}
