#![cfg(feature = "server")]

//! Jev hybrid extraction: judge candidates with Jev, extract per survivor.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use synapse::gateway::Gateway;
use synapse::jev_native::JevNativeProvider;
use synapse::ledger::{InMemoryLedger, LedgerHandle, LedgerStore, UsageEntry};
use synapse::pricing::PricingTable;
use synapse::providers::Catalog;
use synapse::routing::table::RouteTable;
use synapse::server::router;
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ROUTES: &str = r#"
[routes."verify-orgs"]
legs = [
  { provider = "typesafe", model = "jev-latest" },
  { provider = "qwen", model = "qwen-max" },
]
"#;

fn hybrid_body() -> serde_json::Value {
    serde_json::json!({
        "model": "verify-orgs",
        "messages": [{"role": "user", "content": "Resolve Example Ltd"}],
        "jev": {
            "questions": {
                "c0_match": {"type": "noul", "instructions": "Candidate 0 is Example Ltd's own site."},
                "c1_match": {"type": "noul", "instructions": "Candidate 1 is Example Ltd's own site."}
            },
            "extract": {
                "floor": 0.7,
                "candidates": [
                    {"key": "c0", "question": "c0_match", "text": "example ltd official homepage"},
                    {"key": "c1", "question": "c1_match", "text": "a directory listing mentioning example ltd"}
                ],
                "prompt": "Extract the legal name from this candidate.\n\nCandidate {{key}}:\n{{text}}",
                "response_schema": {"type": "object", "properties": {"legalName": {"type": "string"}}, "required": ["legalName"]}
            }
        }
    })
}

fn jev_answers() -> serde_json::Value {
    serde_json::json!({
        "model": "jev-1.13.0",
        "answers": {
            "c0_match": {"type": "noul", "noul": 0.91},
            "c1_match": {"type": "noul", "noul": 0.22}
        },
        "usage": {"input_tokens": 100, "output_tokens": 10}
    })
}

fn extract_sse(legal_name: &str) -> String {
    let payload = serde_json::json!({"legalName": legal_name}).to_string();
    format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n\
         data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}],\"usage\":{{\"prompt_tokens\":30,\"completion_tokens\":7,\"total_tokens\":37}}}}\n\n\
         data: [DONE]\n\n",
        payload.replace('"', "\\\"")
    )
}

async fn gateway(jev_uri: Option<String>, qwen_uri: &str) -> (Gateway, Arc<InMemoryLedger>) {
    let routes = RouteTable::from_toml_str(ROUTES).unwrap();
    let env = HashMap::from([
        ("DASHSCOPE_API_KEY".to_string(), "sk-test".to_string()),
        ("DASHSCOPE_BASE_URL".to_string(), format!("{qwen_uri}/v1")),
        ("TYPESAFE_API_KEY".to_string(), "sk-test".to_string()),
    ]);
    let catalog =
        Catalog::build(&env, &routes.referenced_providers(), Duration::from_secs(5)).unwrap();
    let store = Arc::new(InMemoryLedger::default());
    let mut builder = Gateway::builder()
        .routes(routes)
        .catalog(catalog)
        .pricing(PricingTable::default())
        .ledger(LedgerHandle::spawn(
            store.clone() as Arc<dyn LedgerStore>,
            64,
        ))
        .default_tenant("unattributed");
    if let Some(uri) = jev_uri {
        builder = builder.jev_native(Some(JevNativeProvider::new(
            "sk-test".into(),
            Some(uri),
            Duration::from_secs(5),
        )));
    }
    (builder.build().unwrap(), store)
}

fn request(body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("x-synapse-tenant", "acme")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn ledger_rows(store: &InMemoryLedger, min: usize) -> Vec<UsageEntry> {
    for _ in 0..100 {
        {
            let rows = store.entries.lock();
            if rows.len() >= min {
                return rows.clone();
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    store.entries.lock().clone()
}

#[tokio::test]
async fn one_survivor_is_extracted_once() {
    let jev = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jev_answers()))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("Candidate c0:"))
        .and(body_string_contains("example ltd official homepage"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(extract_sse("Example Ltd")),
        )
        .mount(&qwen)
        .await;

    let (gw, store) = gateway(Some(jev.uri()), &qwen.uri()).await;
    let resp = router(Arc::new(gw))
        .oneshot(request(hybrid_body()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(json["jev"]["survivors"], serde_json::json!(["c0"]));
    assert_eq!(json["jev"]["degraded"], false);
    assert_eq!(json["jev"]["answers"]["c0_match"]["noul"], 0.91);
    assert_eq!(json["model"], "jev-1.13.0");
    let content = json["choices"][0]["message"]["content"].as_str().unwrap();
    let map: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(map["c0"]["legalName"], "Example Ltd");
    assert!(map.get("c1").is_none());
    assert_eq!(json["usage"]["prompt_tokens"], 130); // 100 jev + 30 extraction
    assert_eq!(json["usage"]["completion_tokens"], 17);
    assert_eq!(json["usage"]["total_tokens"], 147);

    let rows = ledger_rows(&store, 2).await;
    assert_eq!(rows.len(), 2);
    let jev_row = rows.iter().find(|r| r.provider == "typesafe").unwrap();
    assert_eq!(jev_row.lane, "jev");
    assert_eq!(jev_row.op, "chat");
    assert_eq!(jev_row.input_tokens, 100);
    assert_eq!(jev_row.tenant, "acme");
    let ext_row = rows.iter().find(|r| r.provider == "qwen").unwrap();
    assert_eq!(ext_row.lane, "standard");
    assert_eq!(ext_row.op, "chat");
    assert_eq!(ext_row.tenant, "acme");
    assert!(!jev_row.request_id.is_empty());
    assert_eq!(jev_row.request_id, ext_row.request_id);
}

#[tokio::test]
async fn noul_equal_to_floor_survives() {
    let jev = MockServer::start().await;
    let mut answers = jev_answers();
    answers["answers"]["c1_match"]["noul"] = serde_json::json!(0.7);
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answers))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(extract_sse("X")),
        )
        .mount(&qwen)
        .await;

    let (gw, _store) = gateway(Some(jev.uri()), &qwen.uri()).await;
    let resp = router(Arc::new(gw))
        .oneshot(request(hybrid_body()))
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["jev"]["survivors"], serde_json::json!(["c0", "c1"]));
    assert_eq!(json["jev"]["degraded"], false);
}

#[tokio::test]
async fn zero_survivors_omit_content() {
    let jev = MockServer::start().await;
    let mut answers = jev_answers();
    answers["answers"]["c0_match"]["noul"] = serde_json::json!(0.1);
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answers))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string("must not run"))
        .expect(0)
        .mount(&qwen)
        .await;

    let (gw, _store) = gateway(Some(jev.uri()), &qwen.uri()).await;
    let resp = router(Arc::new(gw))
        .oneshot(request(hybrid_body()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["jev"]["survivors"], serde_json::json!([]));
    assert_eq!(json["jev"]["degraded"], false);
    assert!(json["choices"][0]["message"].get("content").is_none());
}

#[tokio::test]
async fn jev_exhaustion_extracts_all_candidates_flagged_degraded() {
    let jev = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "error": {"code": 429, "message": "quota"}
        })))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;
    // Both candidates get extracted (ungated) after the Jev lane exhausts.
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(extract_sse("Recovered Ltd")),
        )
        .expect(2)
        .mount(&qwen)
        .await;

    let (gw, _store) = gateway(Some(jev.uri()), &qwen.uri()).await;
    let resp = router(Arc::new(gw))
        .oneshot(request(hybrid_body()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["jev"]["degraded"], true);
    assert_eq!(json["jev"]["answers"], serde_json::json!({}));
    assert_eq!(json["jev"]["survivors"], serde_json::json!(["c0", "c1"]));
    let content = json["choices"][0]["message"]["content"].as_str().unwrap();
    let map: serde_json::Value = serde_json::from_str(content).unwrap();
    assert!(map.get("c0").is_some());
    assert!(map.get("c1").is_some());
}

#[tokio::test]
async fn survivor_extraction_advances_past_a_failing_leg() {
    // leg 1 (qwen) 500s; leg 2 (oai_compat) answers — the extraction must
    // advance, succeed, and NOT mark the response degraded.
    let jev = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jev_answers()))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&qwen)
        .await;
    let oai = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(extract_sse("Advanced Ltd")),
        )
        .mount(&oai)
        .await;

    let routes = r#"
[routes."verify-orgs"]
legs = [
  { provider = "typesafe", model = "jev-latest" },
  { provider = "qwen", model = "qwen-max" },
  { provider = "oai_compat", model = "local-llm" },
]
"#;
    let route_table = RouteTable::from_toml_str(routes).unwrap();
    let env = HashMap::from([
        ("DASHSCOPE_API_KEY".to_string(), "sk-test".to_string()),
        (
            "DASHSCOPE_BASE_URL".to_string(),
            format!("{}/v1", qwen.uri()),
        ),
        (
            "OAI_COMPAT_BASE_URL".to_string(),
            format!("{}/v1", oai.uri()),
        ),
        ("TYPESAFE_API_KEY".to_string(), "sk-test".to_string()),
    ]);
    let catalog = Catalog::build(
        &env,
        &route_table.referenced_providers(),
        Duration::from_secs(5),
    )
    .unwrap();
    let store = Arc::new(InMemoryLedger::default());
    let gw = Gateway::builder()
        .routes(route_table)
        .catalog(catalog)
        .pricing(PricingTable::default())
        .ledger(LedgerHandle::spawn(
            store.clone() as Arc<dyn LedgerStore>,
            64,
        ))
        .jev_native(Some(JevNativeProvider::new(
            "sk-test".into(),
            Some(jev.uri()),
            Duration::from_secs(5),
        )))
        .default_tenant("unattributed")
        .build()
        .unwrap();

    let resp = router(Arc::new(gw))
        .oneshot(request(hybrid_body()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["jev"]["degraded"], false);
    let content = json["choices"][0]["message"]["content"].as_str().unwrap();
    let map: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(map["c0"]["legalName"], "Advanced Ltd");
}

#[tokio::test]
async fn survivor_exhausting_all_chat_legs_is_omitted_and_marks_degraded() {
    let jev = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jev_answers()))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&qwen)
        .await;

    let (gw, _store) = gateway(Some(jev.uri()), &qwen.uri()).await;
    let resp = router(Arc::new(gw))
        .oneshot(request(hybrid_body()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["jev"]["degraded"], true);
    // Extraction ran (c0 attempted) but failed → "{}" present, c0 absent.
    let content = json["choices"][0]["message"]["content"].as_str().unwrap();
    assert_eq!(content, "{}");
}

#[tokio::test]
async fn extract_with_stream_is_rejected() {
    let jev = MockServer::start().await;
    let qwen = MockServer::start().await;
    let (gw, _store) = gateway(Some(jev.uri()), &qwen.uri()).await;
    let mut body = hybrid_body();
    body["stream"] = serde_json::json!(true);
    let resp = router(Arc::new(gw)).oneshot(request(body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("stream"));
}

#[tokio::test]
async fn extract_on_route_without_chat_legs_is_rejected() {
    let routes = r#"
[routes."typesafe-only"]
legs = [{ provider = "typesafe", model = "jev-latest" }]
"#;
    let jev = MockServer::start().await;
    let env = HashMap::from([("TYPESAFE_API_KEY".to_string(), "sk-test".to_string())]);
    let route_table = RouteTable::from_toml_str(routes).unwrap();
    let catalog = Catalog::build(
        &env,
        &route_table.referenced_providers(),
        Duration::from_secs(5),
    )
    .unwrap();
    let store = Arc::new(InMemoryLedger::default());
    let gw = Gateway::builder()
        .routes(route_table)
        .catalog(catalog)
        .pricing(PricingTable::default())
        .ledger(LedgerHandle::spawn(
            store.clone() as Arc<dyn LedgerStore>,
            64,
        ))
        .jev_native(Some(JevNativeProvider::new(
            "sk-test".into(),
            Some(jev.uri()),
            Duration::from_secs(5),
        )))
        .default_tenant("unattributed")
        .build()
        .unwrap();
    let mut body = hybrid_body();
    body["model"] = serde_json::json!("typesafe-only");
    let resp = router(Arc::new(gw)).oneshot(request(body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no chat legs"));
}
