//! Static pricing table + cost calculation.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ModelPrice {
    /// USD per 1M input tokens.
    pub input: f64,
    /// USD per 1M output tokens.
    pub output: f64,
}

#[derive(Debug, Clone, Default)]
pub struct PricingTable {
    prices: HashMap<String, ModelPrice>,
}

impl PricingTable {
    pub fn from_toml_str(s: &str) -> anyhow::Result<Self> {
        let prices: HashMap<String, ModelPrice> = toml::from_str(s)?;
        Ok(Self { prices })
    }

    /// Cost in USD for a completed call. Unknown `provider:model` → 0.0
    /// (self-hosted / unpriced models do not error the request).
    pub fn cost_usd(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> f64 {
        self.prices
            .get(&format!("{provider}:{model}"))
            .map(|p| {
                (input_tokens as f64 * p.input + output_tokens as f64 * p.output) / 1_000_000.0
            })
            .unwrap_or(0.0)
    }

    /// Cost in USD for an embedding call (input tokens only). Unlike `cost_usd`,
    /// an unknown `provider:model` falls back to `default_input_per_mtok` (USD per
    /// 1M tokens) so embedding usage is never silently free.
    pub fn embedding_cost_usd(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        default_input_per_mtok: f64,
    ) -> f64 {
        let per_mtok = self
            .prices
            .get(&format!("{provider}:{model}"))
            .map(|p| p.input)
            .unwrap_or(default_input_per_mtok);
        input_tokens as f64 * per_mtok / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        ["vertex:gemini-3-pro"]
        input = 1.25
        output = 5.0
    "#;

    #[test]
    fn computes_cost_from_tokens() {
        let t = PricingTable::from_toml_str(SAMPLE).unwrap();
        // 1M input @1.25 + 1M output @5.0 = 6.25
        let c = t.cost_usd("vertex", "gemini-3-pro", 1_000_000, 1_000_000);
        assert!((c - 6.25).abs() < 1e-9, "got {c}");
    }

    #[test]
    fn unknown_model_is_free_not_error() {
        let t = PricingTable::from_toml_str(SAMPLE).unwrap();
        assert_eq!(t.cost_usd("oai_compat", "qwen-local", 1000, 1000), 0.0);
    }

    #[test]
    fn embedding_uses_key_when_present_else_default() {
        let t = PricingTable::from_toml_str(
            "[\"vertex:text-embedding-004\"]\ninput = 0.025\noutput = 0.0\n",
        )
        .unwrap();
        // priced key: 1M tokens * 0.025 = 0.025
        assert!(
            (t.embedding_cost_usd("vertex", "text-embedding-004", 1_000_000, 0.10) - 0.025).abs()
                < 1e-9
        );
        // missing key: falls back to the $0.10/1M default
        assert!(
            (t.embedding_cost_usd("openai", "text-embedding-3-large", 1_000_000, 0.10) - 0.10)
                .abs()
                < 1e-9
        );
    }
}
