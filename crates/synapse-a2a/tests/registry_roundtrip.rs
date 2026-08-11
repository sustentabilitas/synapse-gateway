//! Integration test: admin register → public catalog/card/resolve → deregister → 404.

use std::sync::Arc;

use axum::http::StatusCode;
use http_body_util::BodyExt;
use serde_json::Value;
use synapse_a2a::{
    a2a_admin_router, a2a_public_router, A2aCatalog, A2aRegistry, A2aResolveResponse,
};
use tower::ServiceExt as _;

fn sample_register_body() -> String {
    serde_json::json!({
        "id": "ghg-emissions",
        "name": "GHG Emissions",
        "description": "Estimates GHG emissions",
        "endpoint_url": "http://ploutonion/a2a/agents/ghg-emissions",
        "card_url": "http://ploutonion/a2a/agents/ghg-emissions/.well-known/agent-card.json",
        "tags": ["ghg", "emissions"],
        "card": {
            "name": "GHG Emissions",
            "description": "Estimates GHG emissions",
            "url": "http://ploutonion/a2a/agents/ghg-emissions",
            "version": "1.0",
            "skills": []
        }
    })
    .to_string()
}

#[tokio::test]
async fn register_catalog_card_resolve_then_deregister() {
    let registry = Arc::new(A2aRegistry::new());
    let admin = a2a_admin_router(registry.clone());
    let public = a2a_public_router(registry.clone());

    // Register via admin.
    let resp = admin
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/internal/a2a/agents")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(sample_register_body()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let _ = resp.into_body().collect().await;

    // Catalog lists the agent with registered origin URLs.
    let resp = public
        .clone()
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
    assert_eq!(
        catalog.agents[0].card_url,
        "http://ploutonion/a2a/agents/ghg-emissions/.well-known/agent-card.json"
    );

    // Agent card returns stored card JSON.
    let resp = public
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/a2a/agents/ghg-emissions/.well-known/agent-card.json")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let card: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(card["name"], "GHG Emissions");
    assert_eq!(card["url"], "http://ploutonion/a2a/agents/ghg-emissions");

    // Resolve returns endpoint + card.
    let resp = public
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/a2a/agents/ghg-emissions/resolve")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let resolved: A2aResolveResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resolved.id, "ghg-emissions");
    assert_eq!(
        resolved.endpoint_url,
        "http://ploutonion/a2a/agents/ghg-emissions"
    );
    assert_eq!(resolved.card["name"], "GHG Emissions");

    // Deregister via admin.
    let resp = admin
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

    // Resolve returns 404 after deregister.
    let resp = public
        .oneshot(
            axum::http::Request::builder()
                .uri("/a2a/agents/ghg-emissions/resolve")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let _ = resp.into_body().collect().await;
}
