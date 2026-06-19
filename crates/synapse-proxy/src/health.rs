//! Liveness/readiness probes. Readiness flips to 503 once shutdown begins so
//! load balancers drain this instance before in-flight requests finish.

use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::http::StatusCode;

use crate::proxy::AppState;

pub async fn liveness() -> StatusCode {
    StatusCode::OK
}

pub async fn readiness(State(state): State<AppState>) -> StatusCode {
    if state.shutting_down.load(Ordering::SeqCst) {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    }
}
