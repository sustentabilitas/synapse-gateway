//! Static route-alias → AI task type table.
//!
//! Mirrors `PricingTable`: a flat TOML root map, loaded once at startup. The
//! file is keyed by task type so a taxonomy reads as one line per type:
//!
//! ```toml
//! conversation = ["conversation", "planning", "nl-plan"]
//! extraction   = ["extract", "doc-extract"]
//! ```
//!
//! It is inverted to an alias → task lookup at load, so resolution is O(1) on
//! the hot path.

use std::collections::HashMap;

/// Recorded when neither a caller header nor the table supplies a task type.
pub const DEFAULT_AI_TASK_TYPE: &str = "simple";

#[derive(Debug, Clone, Default)]
pub struct AiTaskTypeTable {
    by_alias: HashMap<String, String>,
}

impl AiTaskTypeTable {
    /// Parse `task = [aliases]` and invert to an alias → task map. An alias
    /// claimed by two task types is ambiguous and fails at startup rather than
    /// silently resolving to whichever the map iterated last.
    pub fn from_toml_str(s: &str) -> anyhow::Result<Self> {
        toml::from_str::<HashMap<String, Vec<String>>>(s)?
            .into_iter()
            .flat_map(|(task, aliases)| {
                aliases
                    .into_iter()
                    .map(move |alias| (alias, task.clone()))
                    .collect::<Vec<_>>()
            })
            .try_fold(HashMap::new(), |mut acc, (alias, task)| {
                match acc.insert(alias.clone(), task.clone()) {
                    Some(prev) if prev != task => {
                        // Sorted so the message is stable regardless of map order.
                        let mut pair = [prev, task];
                        pair.sort();
                        let [a, b] = pair;
                        anyhow::bail!(
                            "route alias '{alias}' is mapped to more than one ai task type ('{a}' and '{b}')"
                        )
                    }
                    _ => Ok(acc),
                }
            })
            .map(|by_alias| Self { by_alias })
    }

    /// The task type configured for `alias`, else [`DEFAULT_AI_TASK_TYPE`].
    pub fn resolve(&self, alias: &str) -> &str {
        self.by_alias
            .get(alias)
            .map(String::as_str)
            .unwrap_or(DEFAULT_AI_TASK_TYPE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        conversation = ["conversation", "planning", "nl-plan"]
        extraction = ["extract", "doc-extract"]
    "#;

    #[test]
    fn resolves_every_alias_of_a_task_type() {
        let t = AiTaskTypeTable::from_toml_str(SAMPLE).unwrap();
        assert_eq!(t.resolve("conversation"), "conversation");
        assert_eq!(t.resolve("planning"), "conversation");
        assert_eq!(t.resolve("nl-plan"), "conversation");
        assert_eq!(t.resolve("doc-extract"), "extraction");
    }

    #[test]
    fn unmapped_alias_falls_back_to_simple() {
        let t = AiTaskTypeTable::from_toml_str(SAMPLE).unwrap();
        assert_eq!(t.resolve("fast"), "simple");
    }

    #[test]
    fn empty_table_resolves_everything_to_simple() {
        let t = AiTaskTypeTable::default();
        assert_eq!(t.resolve("conversation"), "simple");
    }

    #[test]
    fn an_alias_under_two_task_types_is_a_startup_error() {
        let err = AiTaskTypeTable::from_toml_str(
            r#"
            conversation = ["planning"]
            extraction = ["planning"]
            "#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("planning"), "got {err}");
        assert!(
            err.contains("conversation") && err.contains("extraction"),
            "got {err}"
        );
    }

    #[test]
    fn an_alias_repeated_under_one_task_type_is_allowed() {
        let t =
            AiTaskTypeTable::from_toml_str(r#"conversation = ["planning", "planning"]"#).unwrap();
        assert_eq!(t.resolve("planning"), "conversation");
    }

    #[test]
    fn malformed_toml_is_an_error() {
        assert!(AiTaskTypeTable::from_toml_str("conversation = 5").is_err());
    }
}
