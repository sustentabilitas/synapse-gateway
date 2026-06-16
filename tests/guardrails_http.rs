//! End-to-end: a route with a blocking policy returns HTTP 400 with the
//! OpenAI-shaped content-policy error before any upstream call is made.

#![cfg(feature = "server")]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt; // for `oneshot`

use synapse::gateway::Gateway;
use synapse::guard::{GuardEngine, GuardrailsConfig};
use synapse::ledger::{InMemoryLedger, LedgerHandle, LedgerStore};
use synapse::pricing::PricingTable;
use synapse::providers::Catalog;
use synapse::routing::table::RouteTable;
use synapse::server::router;

fn blocked_gateway() -> Gateway {
    let routes = RouteTable::from_toml_str(
        r#"[routes."fast"]
           policy = "strict"
           legs = [{ provider = "qwen", model = "qwen-max" }]"#,
    )
    .unwrap();
    let guard = GuardEngine::from_config(
        &GuardrailsConfig::from_toml_str(
            r#"[guardrails.strict]
               scanners = [{ type = "ban_substrings", substrings = ["forbidden"] }]"#,
        )
        .unwrap(),
    )
    .unwrap();
    // Build a catalog pointing at an unreachable upstream — the guardrail fires
    // before any network call, so this URL is never actually contacted.
    let catalog = Catalog::build(
        &HashMap::from([
            ("DASHSCOPE_API_KEY".to_string(), "sk-test".to_string()),
            (
                "DASHSCOPE_BASE_URL".to_string(),
                "http://127.0.0.1:1/v1".to_string(),
            ),
        ]),
        &routes.referenced_providers(),
        Duration::from_secs(5),
    )
    .unwrap();
    let ledger = LedgerHandle::spawn(
        Arc::new(InMemoryLedger::default()) as Arc<dyn LedgerStore>,
        16,
    );
    Gateway::builder()
        .routes(routes)
        .catalog(catalog)
        .pricing(PricingTable::default())
        .ledger(ledger)
        .guard(guard)
        .build()
        .unwrap()
}

#[tokio::test]
async fn blocked_request_returns_400_content_policy_violation() {
    let app = router(Arc::new(blocked_gateway()));
    let body = serde_json::json!({
        "model": "fast",
        "messages": [{ "role": "user", "content": "this is forbidden" }]
    })
    .to_string();
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["type"], "content_policy_violation");
    assert_eq!(json["error"]["code"], "content_blocked");
    assert_eq!(json["error"]["scanners"][0], "ban_substrings");
}
