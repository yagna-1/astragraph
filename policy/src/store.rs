use crate::parser::AgentPolicy;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[derive(Default)]
pub struct PolicyStore {
    policies: HashMap<String, AgentPolicy>,
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

impl PolicyStore {
    pub fn new() -> Self {
        Self {
            policies: HashMap::new(),
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
