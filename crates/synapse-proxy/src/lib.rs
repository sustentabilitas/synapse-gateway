//! synapse-proxy library surface (used by the binary and integration tests).
pub mod config;
pub mod health;
pub mod proxy;

mod router;
pub use router::build_router;
