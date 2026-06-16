//! Bound context: a generalized key→value bag the proxy injects into forwarded
//! requests. A permanent base (static ⊕ env, built at startup with env winning)
//! is overlaid by an optional pushed binding with a TTL; pushed keys win while
//! live and the overlay reverts on expiry. Single active binding.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The resolved view handed to transforms.
#[derive(Debug, Clone, Default)]
pub struct ResolvedContext {
    values: HashMap<String, String>,
}

impl ResolvedContext {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }
}

struct Overlay {
    values: HashMap<String, String>,
    expires_at: Option<Instant>,
}

/// Holds the permanent base and an optional pushed overlay.
pub struct ContextStore {
    base: HashMap<String, String>,
    overlay: Mutex<Option<Overlay>>,
}

impl ContextStore {
    /// `base` is the merged static ⊕ env map (env precedence applied by the caller).
    pub fn new(base: HashMap<String, String>) -> Self {
        Self {
            base,
            overlay: Mutex::new(None),
        }
    }

    /// Replace the overlay with `values`, expiring after `ttl` (None = no expiry).
    pub fn push(&self, values: HashMap<String, String>, ttl: Option<Duration>) {
        let expires_at = ttl.map(|d| Instant::now() + d);
        *self.overlay.lock().unwrap() = Some(Overlay { values, expires_at });
    }

    /// Drop the overlay, reverting to base.
    pub fn clear(&self) {
        *self.overlay.lock().unwrap() = None;
    }

    pub fn resolve(&self) -> ResolvedContext {
        self.resolve_at(Instant::now())
    }

    /// Base overlaid by a live overlay (overlay keys win). Expired overlay is dropped.
    pub fn resolve_at(&self, now: Instant) -> ResolvedContext {
        let mut values = self.base.clone();
        let mut guard = self.overlay.lock().unwrap();
        if let Some(o) = guard.as_ref() {
            if o.expires_at.map(|e| now >= e).unwrap_or(false) {
                *guard = None; // expired → revert to base
            } else {
                for (k, v) in &o.values {
                    values.insert(k.clone(), v.clone());
                }
            }
        }
        ResolvedContext { values }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> HashMap<String, String> {
        HashMap::from([("org".to_string(), "base-org".to_string())])
    }

    #[test]
    fn resolves_base_when_no_overlay() {
        let s = ContextStore::new(base());
        let c = s.resolve();
        assert_eq!(c.get("org"), Some("base-org"));
        assert_eq!(c.get("missing"), None);
    }

    #[test]
    fn live_overlay_overrides_base() {
        let s = ContextStore::new(base());
        s.push(
            HashMap::from([
                ("org".into(), "pushed".into()),
                ("workspace".into(), "ws".into()),
            ]),
            Some(Duration::from_secs(3600)),
        );
        let c = s.resolve();
        assert_eq!(c.get("org"), Some("pushed")); // overlay wins
        assert_eq!(c.get("workspace"), Some("ws"));
    }

    #[test]
    fn expired_overlay_reverts_to_base() {
        let s = ContextStore::new(base());
        let now = Instant::now();
        s.push(
            HashMap::from([("org".into(), "pushed".into())]),
            Some(Duration::from_secs(10)),
        );
        // resolve far in the future → overlay expired
        let c = s.resolve_at(now + Duration::from_secs(20));
        assert_eq!(c.get("org"), Some("base-org"));
    }

    #[test]
    fn clear_drops_overlay() {
        let s = ContextStore::new(base());
        s.push(HashMap::from([("org".into(), "pushed".into())]), None);
        s.clear();
        assert_eq!(s.resolve().get("org"), Some("base-org"));
    }
}
