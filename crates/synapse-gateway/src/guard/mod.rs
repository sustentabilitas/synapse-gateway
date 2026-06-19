//! Configurable input guardrails backed by `llm-guard`.
//! Per-route policy selection with fallback to a `default` policy.
pub mod engine;
pub mod policy;
pub mod scanners;

pub use engine::GuardEngine;
pub use policy::GuardrailsConfig;
