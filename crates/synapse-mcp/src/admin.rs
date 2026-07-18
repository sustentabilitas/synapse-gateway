//! Admin surface: register/deregister upstream MCP servers in the
//! `McpRegistry`. Mirrors `synapse_proxy::admin`'s `bind`/`unbind` shape.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde::Deserialize;

use crate::registry::McpRegistry;

#[derive(Debug, Deserialize)]
pub struct RegisterServerRequest {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

pub fn mcp_admin_router(registry: Arc<McpRegistry>) -> Router {
    Router::new()
        .route("/internal/mcp/servers", post(register_server))
        .route(
            "/internal/mcp/servers/{name}",
            axum::routing::delete(deregister_server),
        )
        .with_state(registry)
}

async fn register_server(
    State(registry): State<Arc<McpRegistry>>,
    Json(req): Json<RegisterServerRequest>,
) -> StatusCode {
    registry.register(req.name, req.url, req.ttl_seconds.map(Duration::from_secs));
    StatusCode::NO_CONTENT
}

async fn deregister_server(
    State(registry): State<Arc<McpRegistry>>,
    Path(name): Path<String>,
) -> StatusCode {
    registry.deregister(&name);
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn register_then_deregister_updates_registry() {
        let registry = Arc::new(McpRegistry::new());
        let app = mcp_admin_router(registry.clone());

        let body = serde_json::json!({ "name": "x", "url": "http://x.local", "ttl_seconds": 3600 })
            .to_string();
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/internal/mcp/servers")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let _ = resp.into_body().collect().await;
        assert_eq!(registry.resolve("x"), Some("http://x.local".to_string()));

        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/internal/mcp/servers/x")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let _ = resp.into_body().collect().await;
        assert_eq!(registry.resolve("x"), None);
    }
}
