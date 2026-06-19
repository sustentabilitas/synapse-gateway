//! Router assembly shared by the binary and tests.
use axum::routing::get;
use axum::Router;

use crate::health;
use crate::proxy::{self, AppState};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz/liveness", get(health::liveness))
        .route("/healthz/readiness", get(health::readiness))
        .fallback(proxy::handler)
        .with_state(state)
}
