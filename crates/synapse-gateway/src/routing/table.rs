//! Route table: client-facing model alias → ordered fallback legs.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct ChainLeg {
    pub provider: String,
    pub model: String,
    /// Optional per-leg region override for the native Vertex lane. When unset,
    /// the lane falls back to the provider's configured region (env
    /// `VERTEX_LOCATION`). Lets a route pin a model to the region that serves it
    /// (e.g. `global` for Gemini 3 previews) without a process-wide env change.
    #[serde(default)]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RouteEntry {
    legs: Vec<ChainLeg>,
    #[serde(default)]
    policy: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RoutesFile {
    routes: HashMap<String, RouteEntry>,
}

#[derive(Debug, Clone)]
pub struct RouteTable {
    routes: HashMap<String, Vec<ChainLeg>>,
    policies: HashMap<String, String>,
}

impl RouteTable {
    pub fn from_toml_str(s: &str) -> anyhow::Result<Self> {
        let file = toml::from_str::<RoutesFile>(s)?;
        let mut routes = HashMap::new();
        let mut policies = HashMap::new();
        for (name, entry) in file.routes {
            if let Some(policy) = entry.policy {
                policies.insert(name.clone(), policy);
            }
            routes.insert(name, entry.legs);
        }
        Ok(Self { routes, policies })
    }

    /// Ordered legs for a model alias, or `None` if the alias is unknown.
    pub fn legs(&self, model: &str) -> Option<&[ChainLeg]> {
        self.routes.get(model).map(Vec::as_slice)
    }

    /// Policy name selected by a route alias, or `None` when unset.
    pub fn policy_of(&self, model: &str) -> Option<&str> {
        self.policies.get(model).map(String::as_str)
    }

    /// All registered aliases (for `/v1/models`), sorted for stable output.
    pub fn aliases(&self) -> Vec<String> {
        let mut v: Vec<String> = self.routes.keys().cloned().collect();
        v.sort();
        v
    }

    /// Provider ids referenced by any leg (for fail-fast credential validation).
    pub fn referenced_providers(&self) -> std::collections::HashSet<String> {
        self.routes
            .values()
            .flatten()
            .map(|l| l.provider.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        [routes."gemini-pro"]
        legs = [
          { provider = "vertex", model = "gemini-3-pro" },
          { provider = "qwen", model = "qwen-max" },
        ]
        [routes."fast"]
        legs = [{ provider = "vertex", model = "gemini-3-flash" }]
    "#;

    #[test]
    fn parses_and_resolves_legs_in_order() {
        let t = RouteTable::from_toml_str(SAMPLE).unwrap();
        let legs = t.legs("gemini-pro").unwrap();
        assert_eq!(legs.len(), 2);
        assert_eq!(
            legs[0],
            ChainLeg {
                provider: "vertex".into(),
                model: "gemini-3-pro".into(),
                ..Default::default()
            }
        );
        assert_eq!(legs[1].provider, "qwen");
        assert!(t.legs("nope").is_none());
    }

    #[test]
    fn parses_optional_per_leg_region() {
        let t = RouteTable::from_toml_str(
            r#"
            [routes."visual"]
            legs = [
              { provider = "vertex", model = "gemini-3.1-pro-preview", region = "global" },
              { provider = "qwen", model = "qwen3-vl-plus" },
            ]
        "#,
        )
        .unwrap();
        let legs = t.legs("visual").unwrap();
        assert_eq!(legs[0].region.as_deref(), Some("global"));
        assert_eq!(legs[1].region, None);
    }

    #[test]
    fn aliases_are_sorted() {
        let t = RouteTable::from_toml_str(SAMPLE).unwrap();
        assert_eq!(
            t.aliases(),
            vec!["fast".to_string(), "gemini-pro".to_string()]
        );
    }

    #[test]
    fn referenced_providers_collected() {
        let t = RouteTable::from_toml_str(SAMPLE).unwrap();
        let p = t.referenced_providers();
        assert!(p.contains("vertex"));
        assert!(p.contains("qwen"));
    }

    #[test]
    fn parses_optional_route_policy_and_defaults_to_none() {
        let t = RouteTable::from_toml_str(
            r#"
            [routes."guarded"]
            policy = "strict"
            legs = [{ provider = "vertex", model = "gemini-3-pro" }]
            [routes."plain"]
            legs = [{ provider = "qwen", model = "qwen-max" }]
        "#,
        )
        .unwrap();
        assert_eq!(t.policy_of("guarded"), Some("strict"));
        assert_eq!(t.policy_of("plain"), None);
        assert_eq!(t.policy_of("missing"), None);
    }
}
