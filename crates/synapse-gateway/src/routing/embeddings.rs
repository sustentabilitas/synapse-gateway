//! Embedding route table: alias → declared output dimension + ordered fallback legs.
use crate::routing::table::ChainLeg;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Deserialize)]
struct EmbeddingEntry {
    dimensions: u32,
    legs: Vec<ChainLeg>,
}

#[derive(Debug, Clone, Deserialize)]
struct EmbeddingsFile {
    #[serde(default)]
    embeddings: HashMap<String, EmbeddingEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct EmbeddingRouteTable {
    routes: HashMap<String, EmbeddingEntry>,
}

impl EmbeddingRouteTable {
    pub fn from_toml_str(s: &str) -> anyhow::Result<Self> {
        let routes = toml::from_str::<EmbeddingsFile>(s)?.embeddings;
        for (alias, e) in &routes {
            anyhow::ensure!(
                e.dimensions > 0,
                "embedding alias '{alias}' has dimensions = 0"
            );
            anyhow::ensure!(!e.legs.is_empty(), "embedding alias '{alias}' has no legs");
        }
        Ok(Self { routes })
    }

    pub fn legs(&self, alias: &str) -> Option<&[ChainLeg]> {
        self.routes.get(alias).map(|e| e.legs.as_slice())
    }

    pub fn dimensions(&self, alias: &str) -> Option<u32> {
        self.routes.get(alias).map(|e| e.dimensions)
    }

    pub fn aliases(&self) -> Vec<String> {
        let mut v: Vec<String> = self.routes.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn referenced_providers(&self) -> HashSet<String> {
        self.routes
            .values()
            .flat_map(|e| &e.legs)
            .map(|l| l.provider.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        [embeddings."default-embed"]
        dimensions = 768
        legs = [
          { provider = "vertex", model = "text-embedding-004" },
          { provider = "openai", model = "text-embedding-3-small" },
        ]
    "#;

    #[test]
    fn parses_dimensions_and_legs() {
        let t = EmbeddingRouteTable::from_toml_str(SAMPLE).unwrap();
        assert_eq!(t.dimensions("default-embed"), Some(768));
        let legs = t.legs("default-embed").unwrap();
        assert_eq!(legs.len(), 2);
        assert_eq!(
            legs[0],
            ChainLeg {
                provider: "vertex".into(),
                model: "text-embedding-004".into(),
                ..Default::default()
            }
        );
        assert!(t.legs("nope").is_none());
        assert!(t.referenced_providers().contains("openai"));
    }

    #[test]
    fn missing_dimensions_is_rejected() {
        let bad = r#"
            [embeddings."x"]
            legs = [{ provider = "vertex", model = "text-embedding-004" }]
        "#;
        assert!(EmbeddingRouteTable::from_toml_str(bad).is_err());
    }

    #[test]
    fn empty_legs_is_rejected() {
        let bad = r#"
            [embeddings."x"]
            dimensions = 768
            legs = []
        "#;
        assert!(EmbeddingRouteTable::from_toml_str(bad).is_err());
    }
}
