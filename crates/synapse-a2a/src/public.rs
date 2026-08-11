//! Public A2A discovery surface: catalog, agent card, and resolve.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::Value;

use crate::registry::{A2aRegistry, RegisteredA2aAgent};
use crate::types::{A2aCatalog, A2aCatalogEntry, A2aResolveResponse};

pub fn a2a_public_router(registry: Arc<A2aRegistry>) -> Router {
    Router::new()
        .route("/.well-known/a2a-agent-catalog.json", get(list_catalog))
        .route(
            "/a2a/agents/{id}/.well-known/agent-card.json",
            get(get_agent_card),
        )
        .route("/a2a/agents/{id}/resolve", get(resolve_agent))
        .with_state(registry)
}

async fn list_catalog(State(registry): State<Arc<A2aRegistry>>) -> Json<A2aCatalog> {
    Json(A2aCatalog {
        version: "1.0".into(),
        agents: registry.list().into_iter().map(to_catalog_entry).collect(),
    })
}

async fn get_agent_card(
    State(registry): State<Arc<A2aRegistry>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    registry
        .resolve(&id)
        .map(|agent| Json(agent.card))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn resolve_agent(
    State(registry): State<Arc<A2aRegistry>>,
    Path(id): Path<String>,
) -> Result<Json<A2aResolveResponse>, StatusCode> {
    registry
        .resolve(&id)
        .map(to_resolve_response)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

fn to_catalog_entry(agent: RegisteredA2aAgent) -> A2aCatalogEntry {
    A2aCatalogEntry {
        id: agent.id,
        name: agent.name,
        description: agent.description,
        card_url: agent.card_url,
        endpoint_url: agent.endpoint_url,
        tags: agent.tags,
    }
}

fn to_resolve_response(agent: RegisteredA2aAgent) -> A2aResolveResponse {
    A2aResolveResponse {
        id: agent.id,
        endpoint_url: agent.endpoint_url,
        card_url: agent.card_url,
        card: agent.card,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn sample_card() -> Value {
        serde_json::json!({
            "name": "GHG",
            "description": "d",
            "url": "http://ploutonion/a2a/agents/ghg-emissions",
            "version": "1.0",
            "skills": []
        })
    }

    fn register_sample(registry: &A2aRegistry) {
        registry.register(
            "ghg-emissions".into(),
            "GHG Emissions".into(),
            "d".into(),
            "http://ploutonion/a2a/agents/ghg-emissions".into(),
            "http://ploutonion/a2a/agents/ghg-emissions/.well-known/agent-card.json".into(),
            vec!["ghg".into()],
            sample_card(),
            None,
        );
    }

    #[tokio::test]
    async fn catalog_lists_registered_agents() {
        let registry = Arc::new(A2aRegistry::new());
        register_sample(&registry);
        let app = a2a_public_router(registry);

        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/.well-known/a2a-agent-catalog.json")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let catalog: A2aCatalog = serde_json::from_slice(&body).unwrap();
        assert_eq!(catalog.version, "1.0");
        assert_eq!(catalog.agents.len(), 1);
        assert_eq!(catalog.agents[0].id, "ghg-emissions");
        assert_eq!(
            catalog.agents[0].endpoint_url,
            "http://ploutonion/a2a/agents/ghg-emissions"
        );
    }

    #[tokio::test]
    async fn card_and_resolve_return_404_when_missing() {
        let registry = Arc::new(A2aRegistry::new());
        let app = a2a_public_router(registry);

        let card_resp = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/a2a/agents/missing/.well-known/agent-card.json")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(card_resp.status(), StatusCode::NOT_FOUND);
        let _ = card_resp.into_body().collect().await;

        let resolve_resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/a2a/agents/missing/resolve")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resolve_resp.status(), StatusCode::NOT_FOUND);
        let _ = resolve_resp.into_body().collect().await;
    }
}
