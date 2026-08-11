//! Admin surface: register/deregister A2A agents in the `A2aRegistry`.
//! Mirrors `synapse_mcp::admin`'s register/deregister shape.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};

use crate::registry::A2aRegistry;
use crate::types::RegisterA2aAgentRequest;

pub fn a2a_admin_router(registry: Arc<A2aRegistry>) -> Router {
    Router::new()
        .route("/internal/a2a/agents", post(register_agent))
        .route(
            "/internal/a2a/agents/{id}",
            axum::routing::delete(deregister_agent),
        )
        .with_state(registry)
}

async fn register_agent(
    State(registry): State<Arc<A2aRegistry>>,
    Json(req): Json<RegisterA2aAgentRequest>,
) -> StatusCode {
    let _ = registry.try_register(
        req.id,
        req.name,
        req.description,
        req.endpoint_url,
        req.card_url,
        req.tags,
        req.card,
        req.ttl_seconds.map(Duration::from_secs),
    );
    StatusCode::NO_CONTENT
}

async fn deregister_agent(
    State(registry): State<Arc<A2aRegistry>>,
    Path(id): Path<String>,
) -> StatusCode {
    registry.deregister(&id);
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn register_then_deregister_updates_registry() {
        let registry = Arc::new(A2aRegistry::new());
        let app = a2a_admin_router(registry.clone());

        let body = serde_json::json!({
            "id": "ghg-emissions",
            "name": "GHG Emissions",
            "description": "d",
            "endpoint_url": "http://ploutonion/a2a/agents/ghg-emissions",
            "card_url": "http://ploutonion/a2a/agents/ghg-emissions/.well-known/agent-card.json",
            "tags": ["ghg"],
            "card": {"name": "GHG", "skills": []},
            "ttl_seconds": 3600
        })
        .to_string();
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/internal/a2a/agents")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let _ = resp.into_body().collect().await;
        assert!(registry.resolve("ghg-emissions").is_some());

        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("DELETE")
                    .uri("/internal/a2a/agents/ghg-emissions")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        let _ = resp.into_body().collect().await;
        assert!(registry.resolve("ghg-emissions").is_none());
    }
}
