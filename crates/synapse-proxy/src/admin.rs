//! Admin surface (separate listener): push/clear the context overlay.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

use crate::context::ContextStore;

#[derive(Debug, Deserialize)]
pub struct BindRequest {
    pub values: HashMap<String, String>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

pub fn admin_router(context: Arc<ContextStore>) -> Router {
    Router::new()
        .route("/internal/bind", post(bind).delete(unbind))
        .with_state(context)
}

async fn bind(State(ctx): State<Arc<ContextStore>>, Json(req): Json<BindRequest>) -> StatusCode {
    ctx.push(req.values, req.ttl_seconds.map(Duration::from_secs));
    StatusCode::NO_CONTENT
}

async fn unbind(State(ctx): State<Arc<ContextStore>>) -> StatusCode {
    ctx.clear();
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn bind_then_unbind_updates_context() {
        let ctx = Arc::new(ContextStore::new(HashMap::new()));
        let app = admin_router(ctx.clone());
        let body =
            serde_json::json!({ "values": { "org": "pushed" }, "ttl_seconds": 3600 }).to_string();
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/internal/bind")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 204);
        let _ = resp.into_body().collect().await;
        assert_eq!(ctx.resolve().get("org"), Some("pushed"));

        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/internal/bind")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 204);
        assert_eq!(ctx.resolve().get("org"), None);
    }
}
