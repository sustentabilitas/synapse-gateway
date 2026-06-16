//! Guardrails config: named policies parsed from `guardrails.toml`.

use serde::Deserialize;
use std::collections::HashMap;

/// Enforcement mode for a policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyMode {
    /// Reject the request when a block-severity scanner fires (default).
    #[default]
    Block,
    /// Never reject; record a would-block and proceed (safe rollout).
    Observe,
}

/// One scanner entry: either a bare name (defaults) or a table with params.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ScannerSpec {
    Named(String),
    Detailed(DetailedScanner),
}

/// Table form of a scanner entry. Only the fields relevant to the named
/// scanner are read; unknown combinations are rejected when the scanner is built.
#[derive(Debug, Clone, Deserialize)]
pub struct DetailedScanner {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub substrings: Vec<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub max_chars: Option<usize>,
    #[serde(default)]
    pub threshold: Option<usize>,
}

/// A named policy: a mode plus an ordered list of scanners.
#[derive(Debug, Clone, Deserialize)]
pub struct Policy {
    #[serde(default)]
    pub mode: PolicyMode,
    pub scanners: Vec<ScannerSpec>,
}

/// Top-level `guardrails.toml`: `[guardrails.<name>]` tables.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GuardrailsConfig {
    #[serde(default)]
    pub guardrails: HashMap<String, Policy>,
}

impl GuardrailsConfig {
    pub fn from_toml_str(s: &str) -> anyhow::Result<Self> {
        toml::from_str(s).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        [guardrails.default]
        scanners = [
          "secrets",
          { type = "token_limit", max_chars = 32000 },
          { type = "ban_substrings", substrings = ["BEGIN RSA PRIVATE KEY"], severity = "block" },
        ]
        [guardrails.canary]
        mode = "observe"
        scanners = ["prompt_injection"]
    "#;

    #[test]
    fn parses_named_and_detailed_scanners_and_modes() {
        let cfg = GuardrailsConfig::from_toml_str(SAMPLE).unwrap();
        let default = cfg.guardrails.get("default").unwrap();
        assert_eq!(default.mode, PolicyMode::Block); // default when omitted
        assert_eq!(default.scanners.len(), 3);
        assert!(matches!(&default.scanners[0], ScannerSpec::Named(n) if n == "secrets"));
        match &default.scanners[1] {
            ScannerSpec::Detailed(d) => {
                assert_eq!(d.kind, "token_limit");
                assert_eq!(d.max_chars, Some(32000));
            }
            _ => panic!("expected detailed scanner"),
        }
        let canary = cfg.guardrails.get("canary").unwrap();
        assert_eq!(canary.mode, PolicyMode::Observe);
    }

    #[test]
    fn empty_config_parses_to_no_policies() {
        let cfg = GuardrailsConfig::from_toml_str("").unwrap();
        assert!(cfg.guardrails.is_empty());
    }
}
