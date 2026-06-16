//! The guard engine: compiles one `llm-guard` pipeline per policy and
//! runs a request's input text through the route's selected policy.

use std::collections::HashMap;
use std::time::Instant;

use llm_guard::{Pipeline, PipelineMode, ScanResult, Severity};
use serde_json::Value;

use crate::error::GatewayError;
use crate::routing::request::{ChatRequest, Message};

use super::policy::{GuardrailsConfig, PolicyMode};
use super::scanners::{build_scanners, BoxedScanner};

struct CompiledPolicy {
    mode: PolicyMode,
    pipeline: Pipeline,
}

/// Holds compiled pipelines keyed by policy name. Construct once at startup
/// and share behind an `Arc`. An empty engine is a no-op (guard returns `Ok`).
#[derive(Default)]
pub struct GuardEngine {
    policies: HashMap<String, CompiledPolicy>,
}

impl GuardEngine {
    /// An engine with no policies — every `guard()` call is a no-op.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Compile every policy in `cfg` into a pipeline. Errors on unknown
    /// scanner names or bad params (fail-fast at startup).
    pub fn from_config(cfg: &GuardrailsConfig) -> anyhow::Result<Self> {
        let mut policies = HashMap::new();
        for (name, policy) in &cfg.guardrails {
            let mut pipeline = Pipeline::new(PipelineMode::All);
            for spec in &policy.scanners {
                for scanner in build_scanners(spec)? {
                    pipeline = pipeline.with(BoxedScanner(scanner));
                }
            }
            policies.insert(
                name.clone(),
                CompiledPolicy {
                    mode: policy.mode,
                    pipeline,
                },
            );
        }
        Ok(Self { policies })
    }

    /// Scan `req`'s input under `policy_name`. Returns `Ok(())` when the
    /// policy is unknown (no-op), clean, in observe mode, or only flags
    /// non-block matches; returns `ContentBlocked` when a block-severity
    /// match fires under a `Block`-mode policy.
    pub fn guard(&self, policy_name: &str, req: &ChatRequest) -> Result<(), GatewayError> {
        let Some(policy) = self.policies.get(policy_name) else {
            return Ok(());
        };
        let text = input_text(req);
        let started = Instant::now();
        let result = policy.pipeline.scan(&text);
        let outcome = outcome_label(&result, policy.mode);
        record_metrics(policy_name, &result, outcome, started);

        if result.should_refuse() && policy.mode == PolicyMode::Block {
            return Err(GatewayError::ContentBlocked {
                policy: policy_name.to_string(),
                scanners: blocking_scanners(&result),
            });
        }
        Ok(())
    }
}

/// Concatenate the text of system/user/tool messages (assistant turns and
/// non-text content parts are skipped) into one buffer to scan.
fn input_text(req: &ChatRequest) -> String {
    req.messages
        .iter()
        .filter(|m| matches!(m.role.as_str(), "system" | "user" | "tool"))
        .filter_map(text_of)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract scannable text from one message: a string content, or the joined
/// `text` fields of an array-of-parts content. `None` when there's no text.
fn text_of(msg: &Message) -> Option<String> {
    match &msg.content {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Array(parts) => {
            let joined = parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            (!joined.is_empty()).then_some(joined)
        }
        _ => None,
    }
}

fn outcome_label(result: &ScanResult, mode: PolicyMode) -> &'static str {
    if !result.flagged() {
        "pass"
    } else if result.should_refuse() {
        match mode {
            PolicyMode::Block => "block",
            PolicyMode::Observe => "observe",
        }
    } else {
        "flag"
    }
}

/// Unique scanner names of block-severity matches (for the error body).
fn blocking_scanners(result: &ScanResult) -> Vec<String> {
    let mut names: Vec<String> = result
        .matches
        .iter()
        .filter(|m| m.severity == Severity::Block)
        .map(|m| m.scanner.to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Info => "info",
        Severity::Warn => "warn",
        Severity::Block => "block",
    }
}

fn record_metrics(policy: &str, result: &ScanResult, outcome: &'static str, started: Instant) {
    metrics::counter!(
        "synapse_guard_scans_total",
        "policy" => policy.to_string(),
        "outcome" => outcome,
    )
    .increment(1);
    for m in &result.matches {
        metrics::counter!(
            "synapse_guard_matches_total",
            "policy" => policy.to_string(),
            "scanner" => m.scanner,
            "severity" => severity_label(m.severity),
        )
        .increment(1);
    }
    metrics::histogram!(
        "synapse_guard_scan_duration_seconds",
        "policy" => policy.to_string(),
    )
    .record(started.elapsed().as_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::policy::GuardrailsConfig;

    fn engine(toml: &str) -> GuardEngine {
        GuardEngine::from_config(&GuardrailsConfig::from_toml_str(toml).unwrap()).unwrap()
    }

    fn req(content: &str) -> ChatRequest {
        serde_json::from_value(serde_json::json!({
            "model": "m",
            "messages": [{ "role": "user", "content": content }]
        }))
        .unwrap()
    }

    const BLOCKING: &str = r#"[guardrails.strict]
        scanners = [{ type = "ban_substrings", substrings = ["forbidden"] }]"#;

    #[test]
    fn unknown_policy_is_noop() {
        let e = engine(BLOCKING);
        assert!(e.guard("nonexistent", &req("forbidden")).is_ok());
    }

    #[test]
    fn clean_input_passes() {
        let e = engine(BLOCKING);
        assert!(e.guard("strict", &req("hello world")).is_ok());
    }

    #[test]
    fn block_mode_refuses_on_block_severity() {
        let e = engine(BLOCKING);
        let err = e.guard("strict", &req("this is forbidden")).unwrap_err();
        match err {
            GatewayError::ContentBlocked { policy, scanners } => {
                assert_eq!(policy, "strict");
                assert_eq!(scanners, vec!["ban_substrings".to_string()]);
            }
            other => panic!("expected ContentBlocked, got {other:?}"),
        }
    }

    #[test]
    fn observe_mode_proceeds_despite_block_severity() {
        let e = engine(r#"[guardrails.canary]
            mode = "observe"
            scanners = [{ type = "ban_substrings", substrings = ["forbidden"] }]"#);
        assert!(e.guard("canary", &req("this is forbidden")).is_ok());
    }

    #[test]
    fn extracts_text_from_array_content_parts() {
        let r: ChatRequest = serde_json::from_value(serde_json::json!({
            "model": "m",
            "messages": [{ "role": "user",
                "content": [{ "type": "text", "text": "forbidden" },
                            { "type": "image_url", "image_url": { "url": "x" } }] }]
        }))
        .unwrap();
        let e = engine(BLOCKING);
        assert!(matches!(
            e.guard("strict", &r).unwrap_err(),
            GatewayError::ContentBlocked { .. }
        ));
    }
}
