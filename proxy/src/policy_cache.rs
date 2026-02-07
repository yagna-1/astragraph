use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub rule_id: String,
    pub threshold: f32,
    pub fallback: i32,
    #[allow(dead_code)]
    pub require_confirmation: bool,
}

#[derive(Default)]
pub struct PolicyCache {
    ttl: Duration,
    entries: HashMap<u64, CacheEntry>,
}

struct CacheEntry {
    decision: PolicyDecision,
    stored_at: Instant,
}

impl PolicyCache {
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            ttl: Duration::from_millis(ttl_ms),
            entries: HashMap::new(),
        }
    }

    pub fn get(&mut self, key: &CacheKey) -> Option<PolicyDecision> {
        let hash = key.hash();
        let entry = self.entries.get(&hash)?;
        if entry.stored_at.elapsed() <= self.ttl {
            Some(entry.decision.clone())
        } else {
            self.entries.remove(&hash);
            None
        }
    }

    pub fn insert(&mut self, key: &CacheKey, decision: PolicyDecision) {
        let hash = key.hash();
        self.entries.insert(
            hash,
            CacheEntry {
                decision,
                stored_at: Instant::now(),
            },
        );
    }
}

pub struct CacheKey<'a> {
    pub agent_id: &'a str,
    pub tool_name: &'a str,
    pub args_json: &'a str,
}

impl<'a> CacheKey<'a> {
    fn hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.agent_id.hash(&mut hasher);
        self.tool_name.hash(&mut hasher);
        self.args_json.hash(&mut hasher);
        hasher.finish()
    }
}
