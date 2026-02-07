use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};

#[derive(Clone, Default)]
pub struct AgentCardCache {
    inner: Arc<RwLock<HashMap<String, u64>>>,
}

impl AgentCardCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&self, agent_id: &str, card_json: &str) -> bool {
        let new_hash = hash_card(card_json);
        let mut map = self.inner.write().expect("agent card cache lock");
        if let Some(old_hash) = map.get(agent_id) {
            if *old_hash != new_hash {
                map.insert(agent_id.to_string(), new_hash);
                return true;
            }
            return false;
        }
        map.insert(agent_id.to_string(), new_hash);
        false
    }
}

fn hash_card(card_json: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    card_json.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::AgentCardCache;

    #[test]
    fn first_seen_card_is_not_marked_changed() {
        let cache = AgentCardCache::new();
        let changed = cache.update("agent-a", r#"{"name":"agent-a"}"#);
        assert!(!changed);
    }

    #[test]
    fn unchanged_card_is_not_marked_changed() {
        let cache = AgentCardCache::new();
        let _ = cache.update("agent-a", r#"{"name":"agent-a"}"#);
        let changed = cache.update("agent-a", r#"{"name":"agent-a"}"#);
        assert!(!changed);
    }

    #[test]
    fn changed_card_is_marked_changed() {
        let cache = AgentCardCache::new();
        let _ = cache.update("agent-a", r#"{"name":"agent-a"}"#);
        let changed = cache.update("agent-a", r#"{"name":"agent-a","version":"2"}"#);
        assert!(changed);
    }
}
