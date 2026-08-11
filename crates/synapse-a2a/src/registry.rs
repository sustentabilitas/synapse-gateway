//! In-memory, TTL-scoped registry of A2A agents.
//!
//! Mirrors the `McpRegistry` pattern in `synapse-mcp`: a sync `RwLock`
//! guards a plain map, expiry is tracked via `Instant`, and a
//! `resolve_at` / `list_at` seam makes TTL expiry deterministically
//! testable. No lock is ever held across an `.await` — all operations
//! here are synchronous by construction.

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
    expires_at: Option<Instant>,
}

/// Registry of A2A agents, keyed by id. Registration is insert-only:
/// the first writer wins; duplicate ids are ignored.
pub struct A2aRegistry {
    inner: RwLock<HashMap<String, RegisteredA2aAgent>>,
}

impl A2aRegistry {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    /// Insert-only registration. Returns `true` if inserted, `false` if `id`
    /// already present (existing entry is left unchanged).
    pub fn try_register(
        &self,
        id: String,
        name: String,
        description: String,
        endpoint_url: String,
        card_url: String,
        tags: Vec<String>,
        card: Value,
        ttl: Option<Duration>,
    ) -> bool {
        let mut guard = self.inner.write().unwrap();
        if guard.contains_key(&id) {
            return false;
        }
        guard.insert(
            id.clone(),
            RegisteredA2aAgent {
                id,
                name,
                description,
                endpoint_url,
                card_url,
                tags,
                card,
                expires_at: expiry_from_ttl(ttl),
            },
        );
        true
    }

    /// Remove an agent by id. No-op if absent.
    pub fn deregister(&self, id: &str) {
        self.inner.write().unwrap().remove(id);
    }

    /// Resolve an agent by id if present and unexpired. An expired entry
    /// is lazily dropped from the registry.
    pub fn resolve(&self, id: &str) -> Option<RegisteredA2aAgent> {
        self.resolve_at(id, Instant::now())
    }

    /// Test seam: resolve as of `now` instead of `Instant::now()`.
    pub fn resolve_at(&self, id: &str, now: Instant) -> Option<RegisteredA2aAgent> {
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
        self.list_at(Instant::now())
    }

    /// Test seam: list as of `now` instead of `Instant::now()`.
    pub fn list_at(&self, now: Instant) -> Vec<RegisteredA2aAgent> {
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
fn expiry_from_ttl(ttl: Option<Duration>) -> Option<Instant> {
    ttl.map(|d| Instant::now() + d)
}

fn is_expired(agent: &RegisteredA2aAgent, now: Instant) -> bool {
    agent.expires_at.map(|e| now >= e).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register_sample(r: &A2aRegistry, id: &str, endpoint_url: &str, ttl: Option<Duration>) {
        assert!(r.try_register(
            id.into(),
            id.into(),
            "d".into(),
            endpoint_url.into(),
            format!("http://gateway/{id}"),
            vec![],
            serde_json::json!({}),
            ttl,
        ));
    }

    #[test]
    fn register_resolve_and_list() {
        let r = A2aRegistry::new();
        let card = serde_json::json!({"name":"GHG","description":"d","url":"http://p/a2a/agents/ghg","version":"1.0","skills":[]});
        assert!(r.try_register(
            "ghg-emissions".into(),
            "GHG Emissions".into(),
            "d".into(),
            "http://ploutonion/a2a/agents/ghg-emissions".into(),
            "http://gateway/a2a/agents/ghg-emissions/.well-known/agent-card.json".into(),
            vec!["ghg".into()],
            card,
            None,
        ));
        assert_eq!(
            r.resolve("ghg-emissions").map(|a| a.endpoint_url.clone()),
            Some("http://ploutonion/a2a/agents/ghg-emissions".into())
        );
        assert_eq!(r.list().len(), 1);
    }

    #[test]
    fn resolve_unknown_id_returns_none() {
        let r = A2aRegistry::new();
        assert!(r.resolve("missing").is_none());
    }

    #[test]
    fn try_register_same_id_is_ignored() {
        let r = A2aRegistry::new();
        assert!(r.try_register(
            "ghg".into(),
            "ghg".into(),
            "d".into(),
            "http://old/a2a".into(),
            "http://gateway/ghg".into(),
            vec![],
            serde_json::json!({"v": 1}),
            None,
        ));
        assert!(!r.try_register(
            "ghg".into(),
            "ghg".into(),
            "d".into(),
            "http://new/a2a".into(),
            "http://gateway/ghg".into(),
            vec![],
            serde_json::json!({"v": 2}),
            None,
        ));
        let agent = r.resolve("ghg").unwrap();
        assert_eq!(agent.endpoint_url, "http://old/a2a");
        assert_eq!(agent.card["v"], 1);
        assert_eq!(r.list().len(), 1);
    }

    #[test]
    fn deregister_then_resolve_returns_none() {
        let r = A2aRegistry::new();
        register_sample(&r, "ghg", "http://p/a2a", None);
        r.deregister("ghg");
        assert!(r.resolve("ghg").is_none());
    }

    #[test]
    fn ttl_expiry_via_resolve_at() {
        let r = A2aRegistry::new();
        let now = Instant::now();
        register_sample(
            &r,
            "alpha",
            "http://alpha.local/a2a",
            Some(Duration::from_secs(10)),
        );

        // Before expiry: resolves.
        assert_eq!(
            r.resolve_at("alpha", now + Duration::from_secs(5))
                .map(|a| a.endpoint_url),
            Some("http://alpha.local/a2a".into())
        );

        // After expiry: None, and the entry is dropped from the map.
        assert!(r
            .resolve_at("alpha", now + Duration::from_secs(20))
            .is_none());
        assert!(!r.inner.read().unwrap().contains_key("alpha"));
    }

    #[test]
    fn ttl_expiry_via_list_at() {
        let r = A2aRegistry::new();
        let now = Instant::now();
        register_sample(
            &r,
            "alpha",
            "http://alpha.local/a2a",
            Some(Duration::from_secs(10)),
        );

        assert_eq!(r.list_at(now + Duration::from_secs(5)).len(), 1);
        assert_eq!(r.list_at(now + Duration::from_secs(20)).len(), 0);
        assert!(!r.inner.read().unwrap().contains_key("alpha"));
    }
}
