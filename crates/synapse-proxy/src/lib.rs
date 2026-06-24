//! synapse-proxy library surface (used by the binary and integration tests).
pub mod admin;
pub mod builder;
pub mod config;
pub mod context;
pub mod health;
pub mod http_client;
pub mod metrics;
pub mod proxy;
pub mod transform;

mod router;
pub use builder::ProxyBuilder;
pub use router::{build_router, build_router_from_config};
