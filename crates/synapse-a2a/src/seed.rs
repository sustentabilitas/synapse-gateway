//! Boot-time seed of `A2aRegistry` from TOML + HTTP card fetch.

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct A2aSeedFile {
    #[serde(default)]
    pub a2a_agents: Vec<A2aSeedAgent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct A2aSeedAgent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub card_url: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

pub fn parse_seed_toml(raw: &str) -> Result<A2aSeedFile> {
    toml::from_str(raw).context("parsing a2a seed TOML")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_seed_toml_reads_agents() {
        let raw = r#"
[[a2a_agents]]
id = "ghg-emissions"
name = "GHG Emissions"
description = "Estimates GHG emissions"
endpoint_url = "http://ploutonion/a2a/agents/ghg-emissions"
card_url = "http://ploutonion/card.json"
tags = ["ghg"]
ttl_seconds = 3600
"#;
        let file = parse_seed_toml(raw).unwrap();
        assert_eq!(file.a2a_agents.len(), 1);
        assert_eq!(file.a2a_agents[0].id, "ghg-emissions");
        assert_eq!(file.a2a_agents[0].ttl_seconds, Some(3600));
    }

    #[test]
    fn parse_empty_or_comment_only_yields_zero_agents() {
        assert!(parse_seed_toml("").unwrap().a2a_agents.is_empty());
        assert!(parse_seed_toml("# only comments\n").unwrap().a2a_agents.is_empty());
    }

    #[test]
    fn parse_invalid_toml_errors() {
        assert!(parse_seed_toml("[[a2a_agents]\n").is_err());
    }
}
