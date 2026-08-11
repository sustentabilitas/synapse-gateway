//! Seed via HTTP card fetch, then admin-register a second agent; both in catalog.

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use http_body_util::BodyExt;
use synapse_a2a::{
    a2a_admin_router, a2a_public_router, parse_seed_toml, seed_agents, A2aCatalog, A2aRegistry,
    A2aSeedAgent,
};
use tower::ServiceExt as _;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn seed_then_admin_add_both_in_catalog() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ghg-card.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "name": "GHG Emissions",
            "description": "d",
            "url": "http://ploutonion/a2a/agents/ghg-emissions",
            "version": "1.0",
            "skills": []
        })))
        .mount(&server)
        .await;

    let registry = Arc::new(A2aRegistry::new());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();
    let seeded = A2aSeedAgent {
        id: "ghg-emissions".into(),
        name: "GHG Emissions".into(),
        description: "d".into(),
        endpoint_url: "http://ploutonion/a2a/agents/ghg-emissions".into(),
        card_url: format!("{}/ghg-card.json", server.uri()),
        tags: vec!["ghg".into()],
        ttl_seconds: None,
    };
    seed_agents(&registry, &[seeded], &client).await.unwrap();

    let admin = a2a_admin_router(registry.clone());
    let public = a2a_public_router(registry.clone());

    let body = serde_json::json!({
        "id": "waste-tracker",
        "name": "Waste Tracker",
        "description": "d",
        "endpoint_url": "http://ploutonion/a2a/agents/waste-tracker",
        "card_url": "http://ploutonion/a2a/agents/waste-tracker/.well-known/agent-card.json",
        "tags": ["waste"],
        "card": {
            "name": "Waste Tracker",
            "description": "d",
            "url": "http://ploutonion/a2a/agents/waste-tracker",
            "version": "1.0",
            "skills": []
        }
    })
    .to_string();
    let resp = admin
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

    // Re-POST seeded id must not overwrite.
    let overwrite = serde_json::json!({
        "id": "ghg-emissions",
        "name": "Hijack",
        "description": "x",
        "endpoint_url": "http://evil/a2a",
        "card_url": "http://evil/card.json",
        "tags": [],
        "card": {"name": "Hijack"}
    })
    .to_string();
    let resp = a2a_admin_router(registry.clone())
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/internal/a2a/agents")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(overwrite))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let _ = resp.into_body().collect().await;

    let resp = public
        .oneshot(
            axum::http::Request::builder()
                .uri("/.well-known/a2a-agent-catalog.json")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let catalog: A2aCatalog = serde_json::from_slice(&bytes).unwrap();
    let mut ids: Vec<_> = catalog.agents.iter().map(|a| a.id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, vec!["ghg-emissions", "waste-tracker"]);
    assert_eq!(
        registry.resolve("ghg-emissions").unwrap().name,
        "GHG Emissions"
    );

    let _ = parse_seed_toml("").unwrap();
}
