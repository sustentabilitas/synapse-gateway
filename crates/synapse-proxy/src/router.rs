//! Router assembly shared by the binary and tests.
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::routing::get;
use axum::Router;

use crate::builder::ProxyBuilder;
use crate::health;
use crate::proxy::{self, AppState};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz/liveness", get(health::liveness))
        .route("/healthz/readiness", get(health::readiness))
        .fallback(proxy::handler)
        .with_state(state)
}

/// Build the data-plane router from a builder (used by integration tests).
pub fn build_router_from_config(builder: ProxyBuilder) -> anyhow::Result<Router> {
    let built = builder.build()?;
    let (metrics, _registry) = crate::metrics::Metrics::new()?;
    let state = AppState {
        routes: Arc::new(built.routes),
        context: built.context,
        client: crate::http_client::build_http_client()?,
        shutting_down: Arc::new(AtomicBool::new(false)),
        metrics: Arc::new(metrics),
    };
    Ok(build_router(state))
}
