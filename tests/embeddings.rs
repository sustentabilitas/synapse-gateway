#![cfg(feature = "server")]

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use synapse::embeddings::{EmbedOut, EmbeddingProvider};
use synapse::error::GatewayError;
use synapse::ledger::{InMemoryLedger, LedgerHandle, LedgerStore};
use synapse::pricing::PricingTable;
use synapse::providers::Catalog;
use synapse::routing::embeddings::EmbeddingRouteTable;
use synapse::routing::table::RouteTable;
use synapse::server::router;
use tower::ServiceExt;

/// One zero-vector per input (length `dims`) plus a fixed input-token count.
struct StubEmbedder;

#[async_trait]
impl EmbeddingProvider for StubEmbedder {
    async fn embed(
        &self,
        _model: &str,
        inputs: &[String],
        dims: u32,
    ) -> Result<EmbedOut, GatewayError> {
        Ok(EmbedOut {
            vectors: inputs.iter().map(|_| vec![0.0f32; dims as usize]).collect(),
            input_tokens: 7,
        })
    }
}

#[tokio::test]
async fn http_embeddings_returns_list_and_records_ledger() {
    let embed_routes = EmbeddingRouteTable::from_toml_str(
        r#"
        [embeddings."test-embed"]
        dimensions = 4
        legs = [{ provider = "stub", model = "stub-embed" }]
        "#,
    )
    .unwrap();

    // Chat routing is required by the builder even though we only exercise embeddings.
    let routes = RouteTable::from_toml_str(
        r#"[routes."fast"]
           legs = [{ provider = "qwen", model = "qwen-max" }]"#,
    )
    .unwrap();
    let catalog = Catalog::build(
        &std::collections::HashMap::from([("DASHSCOPE_API_KEY".to_string(), "sk".to_string())]),
        &routes.referenced_providers(),
        std::time::Duration::from_secs(5),
    )
    .unwrap();

    let store = Arc::new(InMemoryLedger::default());
    let ledger = LedgerHandle::spawn(store.clone() as Arc<dyn LedgerStore>, 64);
    let gateway = synapse::gateway::Gateway::builder()
        .routes(routes)
        .catalog(catalog)
        .pricing(PricingTable::default())
        .ledger(ledger)
        .default_tenant("unattributed")
        .embed_routes(embed_routes)
        .embedder("stub", Arc::new(StubEmbedder) as Arc<dyn EmbeddingProvider>)
        .build()
        .unwrap();

    let resp = router(Arc::new(gateway))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/embeddings")
                .header("content-type", "application/json")
                .header("x-synapse-tenant", "acme")
                .body(Body::from(r#"{"input":["a","b"],"model":"test-embed"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["object"], "list");
    let data = v["data"].as_array().unwrap();
    assert_eq!(data.len(), 2);
    for (i, d) in data.iter().enumerate() {
        assert_eq!(d["index"], i);
        assert_eq!(d["embedding"].as_array().unwrap().len(), 4);
    }

    // ledger is fire-and-forget; wait briefly for the background writer to drain
    for _ in 0..50 {
        if store.entries.lock().len() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let entries = store.entries.lock();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tenant, "acme");
    assert_eq!(entries[0].op, "embedding");
}
