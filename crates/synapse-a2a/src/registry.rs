//! In-memory, TTL-scoped registry of A2A agents.
//!
//! Mirrors the `McpRegistry` pattern in `synapse-mcp`: a sync `RwLock`
//! guards a plain map, expiry is tracked via `Instant`, and no lock is
//! held across an `.await` — all operations here are synchronous by
//! construction.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde_json::Value;

/// A single registered A2A agent, including its agent-card JSON.
#[derive(Debug, Clone)]
pub struct RegisteredA2aAgent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub card_url: String,
    pub tags: Vec<String>,
    pub card: Value,
    pub expires_at: Option<Instant>,
}

/// Registry of A2A agents, keyed by id. Registering an id that already
/// exists replaces the prior entry (hot-swap) rather than erroring or
/// duplicating.
pub struct A2aRegistry {
    inner: RwLock<HashMap<String, RegisteredA2aAgent>>,
}

impl A2aRegistry {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Register (or hot-swap replace) an agent.
    pub fn register(&self, agent: RegisteredA2aAgent) {
        self.inner.write().unwrap().insert(agent.id.clone(), agent);
    }

    /// Remove an agent by id. No-op if absent.
    pub fn deregister(&self, id: &str) {
        self.inner.write().unwrap().remove(id);
    }

    /// Resolve an agent by id if present and unexpired. An expired entry
    /// is lazily dropped from the registry.
    pub fn resolve(&self, id: &str) -> Option<RegisteredA2aAgent> {
        let now = Instant::now();
        let mut guard = self.inner.write().unwrap();
        match guard.get(id) {
            Some(entry) if is_expired(entry, now) => {
                guard.remove(id);
                None
            }
            Some(entry) => Some(entry.clone()),
            None => None,
        }
    }

    /// List all unexpired agents. Expired entries are lazily dropped.
    pub fn list(&self) -> Vec<RegisteredA2aAgent> {
        let now = Instant::now();
        let mut guard = self.inner.write().unwrap();
        guard.retain(|_, agent| !is_expired(agent, now));
        guard.values().cloned().collect()
    }
}

impl Default for A2aRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert an optional TTL into an absolute `expires_at` instant.
pub fn expiry_from_ttl(ttl: Option<Duration>) -> Option<Instant> {
    ttl.map(|d| Instant::now() + d)
}

fn is_expired(agent: &RegisteredA2aAgent, now: Instant) -> bool {
    agent.expires_at.map(|e| now >= e).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_resolve_and_list() {
        let r = A2aRegistry::new();
        let card = serde_json::json!({"name":"GHG","description":"d","url":"http://p/a2a/agents/ghg","version":"1.0","skills":[]});
        r.register(RegisteredA2aAgent {
            id: "ghg-emissions".into(),
            name: "GHG Emissions".into(),
            description: "d".into(),
            endpoint_url: "http://ploutonion/a2a/agents/ghg-emissions".into(),
            card_url: "http://gateway/a2a/agents/ghg-emissions/.well-known/agent-card.json".into(),
            tags: vec!["ghg".into()],
            card,
            expires_at: None,
        });
        assert_eq!(
            r.resolve("ghg-emissions").map(|a| a.endpoint_url.clone()),
            Some("http://ploutonion/a2a/agents/ghg-emissions".into())
        );
        assert_eq!(r.list().len(), 1);
    }

    #[tokio::test]
    async fn expired_agents_filtered_from_resolve_and_list() {
        let r = A2aRegistry::new();
        let card = serde_json::json!({"name":"Stale"});
        r.register(RegisteredA2aAgent {
            id: "stale".into(),
            name: "Stale".into(),
            description: "expired".into(),
            endpoint_url: "http://example/a2a/agents/stale".into(),
            card_url: "http://gateway/a2a/agents/stale/.well-known/agent-card.json".into(),
            tags: vec![],
            card,
            expires_at: Some(Instant::now() - Duration::from_secs(1)),
        });
        assert!(r.resolve("stale").is_none());
        assert_eq!(r.list().len(), 0);
    }

    #[tokio::test]
    async fn re_register_same_id_hot_swaps() {
        let r = A2aRegistry::new();
        let card = serde_json::json!({});
        r.register(RegisteredA2aAgent {
            id: "ghg".into(),
            name: "Old".into(),
            description: "d".into(),
            endpoint_url: "http://old/a2a".into(),
            card_url: "http://gateway/old".into(),
            tags: vec![],
            card: card.clone(),
            expires_at: None,
        });
        r.register(RegisteredA2aAgent {
            id: "ghg".into(),
            name: "New".into(),
            description: "d".into(),
            endpoint_url: "http://new/a2a".into(),
            card_url: "http://gateway/new".into(),
            tags: vec![],
            card,
            expires_at: None,
        });
        assert_eq!(
            r.resolve("ghg").map(|a| a.endpoint_url.clone()),
            Some("http://new/a2a".into())
        );
        assert_eq!(r.list().len(), 1);
    }

    #[tokio::test]
    async fn deregister_then_resolve_returns_none() {
        let r = A2aRegistry::new();
        r.register(RegisteredA2aAgent {
            id: "ghg".into(),
            name: "GHG".into(),
            description: "d".into(),
            endpoint_url: "http://p/a2a".into(),
            card_url: "http://g/card".into(),
            tags: vec![],
            card: serde_json::json!({}),
            expires_at: None,
        });
        r.deregister("ghg");
        assert!(r.resolve("ghg").is_none());
    }

    #[test]
    fn expiry_from_ttl_none_is_none() {
        assert!(expiry_from_ttl(None).is_none());
    }

    #[test]
    fn expiry_from_ttl_some_is_in_future() {
        let before = Instant::now();
        let expires = expiry_from_ttl(Some(Duration::from_secs(60))).unwrap();
        assert!(expires > before);
    }
}
