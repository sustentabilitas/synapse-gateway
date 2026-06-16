//! Configurable input guardrails backed by `llm-guard`.
//! Per-route policy selection with fallback to a `default` policy.
pub mod policy;

pub use policy::GuardrailsConfig;
