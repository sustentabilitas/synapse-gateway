//! Re-export of the shared `synapse-context` crate.
//!
//! The actual `ContextStore`/`ResolvedContext` implementation lives in the
//! leaf crate `synapse-context` so that `synapse-mcp` can depend on it
//! directly (it needs the exact same type, not a fork) without creating a
//! cyclic package dependency: `synapse-proxy`'s binary depends on
//! `synapse-mcp` (to mount the MCP gateway listener), so `synapse-mcp`
//! cannot in turn depend on `synapse-proxy`. Re-exporting here keeps every
//! existing `synapse_proxy::context::{ContextStore, ResolvedContext}` import
//! path unchanged.
pub use synapse_context::{ContextStore, ResolvedContext};
