#![cfg(feature = "server")]

//! Jev chat lane: a route alias whose `typesafe` legs answer a chat request's
//! `jev` extension block (state + typed questions), with chat legs (openai
//! compatible / native Vertex) as fallback when every Jev leg fails retryably.

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
use synapse::providers::vertex_auth::VertexAuth;
use synapse::providers::Catalog;
use synapse::routing::table::RouteTable;
use synapse::server::router;
use synapse::vertex_native::VertexNativeProvider;
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TICKET: &str = "Customer cannot connect Stripe for 3 days.";

fn qwen_ok(content: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "x", "object": "chat.completion", "created": 0, "model": "qwen-max",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 5, "total_tokens": 8}
    })
}

fn jev_ok() -> serde_json::Value {
    serde_json::json!({
        "model": "jev-1.13.0",
        "answers": {
            "department": {"type": "choice", "choice": "technical", "confidence": 0.78},
            "is_urgent": {"type": "noul", "noul": 1.0}
        },
        "usage": {"input_tokens": 120, "output_tokens": 40}
    })
}

fn chat_body(model: &str, jev: serde_json::Value) -> Request<Body> {
    let mut body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": TICKET}],
    });
    if let Some(obj) = body.as_object_mut() {
        obj.extend(jev.as_object().cloned().unwrap_or_default());
    }
    Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("x-synapse-tenant", "acme")
        .header("x-synapse-workspace", "ws-9")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn jev_block() -> serde_json::Value {
    serde_json::json!({
        "jev": {
            "questions": {
                "department": {
                    "type": "choice",
                    "instructions": "Which team should handle this",
                    "criteria": {"billing": "Payment issues", "technical": "Integration problems"}
                },
                "is_urgent": {"type": "noul", "instructions": "Is this urgent?"}
            }
        }
    })
}

fn gateway(
    routes_toml: &str,
    jev_uri: Option<String>,
    qwen_uri: &str,
    vertex_uri: Option<String>,
) -> (Gateway, Arc<InMemoryLedger>) {
    let routes = RouteTable::from_toml_str(routes_toml).unwrap();
    let env = HashMap::from([
        ("DASHSCOPE_API_KEY".to_string(), "sk-test".to_string()),
        ("DASHSCOPE_BASE_URL".to_string(), format!("{qwen_uri}/v1")),
        ("TYPESAFE_API_KEY".to_string(), "sk-test".to_string()),
        ("VERTEX_PROJECT_ID".to_string(), "p".to_string()),
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
    if let Some(uri) = vertex_uri {
        builder = builder.vertex_native(Some(VertexNativeProvider::new(
            Arc::new(VertexAuth::with_fetcher(|| {
                Box::pin(async { Ok(("test-token".into(), Duration::from_secs(3600))) })
            })),
            "p".into(),
            "global".into(),
            Duration::from_secs(5),
            Some(uri),
        )));
    }
    (builder.build().unwrap(), store)
}

async fn ledger_rows(store: &InMemoryLedger) -> Vec<UsageEntry> {
    for _ in 0..100 {
        {
            let rows = store.entries.lock();
            if !rows.is_empty() {
                return rows.clone();
            }
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    store.entries.lock().clone()
}

const MIXED_ROUTES: &str = r#"
[routes."ticket-triage"]
legs = [
  { provider = "typesafe", model = "jev-latest" },
  { provider = "qwen", model = "qwen-max" },
]
"#;

#[tokio::test]
async fn jev_leg_answers_and_chat_fallback_is_not_called() {
    let jev = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer sk-test"))
        .and(body_string_contains(TICKET)) // default state = serialized messages
        .and(body_string_contains("\"model\":\"jev-latest\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(jev_ok()))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(qwen_ok("must not run")))
        .expect(0)
        .mount(&qwen)
        .await;

    let (gw, store) = gateway(MIXED_ROUTES, Some(jev.uri()), &qwen.uri(), None);
    let resp = router(Arc::new(gw))
        .oneshot(chat_body("ticket-triage", jev_block()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // The decision is the message content, JSON-encoded; response model is
    // the served Jev build.
    let content = json["choices"][0]["message"]["content"].as_str().unwrap();
    let answers: serde_json::Value = serde_json::from_str(content).unwrap();
    assert_eq!(answers["department"]["choice"], "technical");
    assert_eq!(json["model"], "jev-1.13.0");
    assert_eq!(json["usage"]["prompt_tokens"], 120);
    assert_eq!(json["usage"]["completion_tokens"], 40);

    let rows = ledger_rows(&store).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tenant, "acme");
    assert_eq!(rows[0].provider, "typesafe");
    assert_eq!(rows[0].lane, "jev");
    assert_eq!(rows[0].route, "ticket-triage");
    assert_eq!(rows[0].input_tokens, 120);
    assert_eq!(rows[0].output_tokens, 40);
    assert_eq!(rows[0].status, "ok");
}

#[tokio::test]
async fn retryable_jev_failure_falls_back_to_chat_leg() {
    let jev = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": {"code": 500, "message": "internal"}
        })))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;
    // The buffered executor streams providers internally, so the mock answers
    // with OpenAI SSE, not a plain JSON completion.
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"plain chat fallback\"}}]}\n\n\
               data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":5,\"total_tokens\":8}}\n\n\
               data: [DONE]\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse),
        )
        .mount(&qwen)
        .await;

    let (gw, store) = gateway(MIXED_ROUTES, Some(jev.uri()), &qwen.uri(), None);
    let resp = router(Arc::new(gw))
        .oneshot(chat_body("ticket-triage", jev_block()))
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        status,
        StatusCode::OK,
        "body: {}",
        String::from_utf8_lossy(&body)
    );
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Fallback answers as a normal chat completion — the response shape
    // changes with the lane; clients branch on it.
    assert_eq!(
        json["choices"][0]["message"]["content"],
        "plain chat fallback"
    );

    let rows = ledger_rows(&store).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].provider, "qwen");
    assert_eq!(rows[0].lane, "standard");
    assert_eq!(rows[0].output_tokens, 5);
}

#[tokio::test]
async fn non_retryable_jev_error_aborts_without_fallback() {
    let jev = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({
            "error": {"code": "invalid_question", "message": "no criteria"}
        })))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(qwen_ok("must not run")))
        .expect(0)
        .mount(&qwen)
        .await;

    let (gw, store) = gateway(MIXED_ROUTES, Some(jev.uri()), &qwen.uri(), None);
    let resp = router(Arc::new(gw))
        .oneshot(chat_body("ticket-triage", jev_block()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("typesafe 422"));

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(store.entries.lock().is_empty());
}

#[tokio::test]
async fn typesafe_legs_without_jev_block_is_a_clear_400() {
    let jev = MockServer::start().await;
    let qwen = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(qwen_ok("must not run")))
        .expect(0)
        .mount(&qwen)
        .await;

    let (gw, store) = gateway(MIXED_ROUTES, Some(jev.uri()), &qwen.uri(), None);
    let resp = router(Arc::new(gw))
        .oneshot(chat_body("ticket-triage", serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "invalid_request_error");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("'jev' extension block"));

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(store.entries.lock().is_empty());
}

#[tokio::test]
async fn stream_true_emits_the_decision_as_a_single_sse_chunk() {
    let jev = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(200).set_body_json(jev_ok()))
        .mount(&jev)
        .await;
    let qwen = MockServer::start().await;

    let (gw, store) = gateway(MIXED_ROUTES, Some(jev.uri()), &qwen.uri(), None);
    let mut body = jev_block();
    body.as_object_mut()
        .unwrap()
        .insert("stream".into(), serde_json::Value::Bool(true));
    let resp = router(Arc::new(gw))
        .oneshot(chat_body("ticket-triage", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("department"),
        "SSE carries the decision: {text}"
    );
    assert!(text.contains("[DONE]"));

    let rows = ledger_rows(&store).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].lane, "jev");
}

#[tokio::test]
async fn jev_exhaustion_falls_back_to_native_vertex_when_configured() {
    let routes = r#"
[routes."schema-triage"]
legs = [
  { provider = "typesafe", model = "jev-latest" },
  { provider = "vertex", model = "gemini-2.5-flash" },
]
"#;
    let jev = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
            "error": {"code": 429, "message": "quota"}
        })))
        .mount(&jev)
        .await;
    let vertex = MockServer::start().await;
    let sse = "data: {\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"vertex native fallback\"}]}}]}\n\n\
               data: {\"candidates\":[{\"finishReason\":\"STOP\",\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"!\"}]}}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6}}\n\n";
    Mock::given(method("POST"))
        .and(path(
            "/v1/projects/p/locations/global/publishers/google/models/gemini-2.5-flash:streamGenerateContent",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse),
        )
        .mount(&vertex)
        .await;
    let qwen = MockServer::start().await;

    let (gw, store) = gateway(routes, Some(jev.uri()), &qwen.uri(), Some(vertex.uri()));
    // The vertex extension block stays in force for the fallback leg.
    let mut body = jev_block();
    body.as_object_mut().unwrap().insert(
        "vertex".into(),
        serde_json::json!({"response_schema": {"type": "object"}}),
    );
    let resp = router(Arc::new(gw))
        .oneshot(chat_body("schema-triage", body))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["choices"][0]["message"]["content"],
        "vertex native fallback!"
    );

    let rows = ledger_rows(&store).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].provider, "vertex");
    assert_eq!(rows[0].lane, "native");
    assert_eq!(rows[0].input_tokens, 4);
    assert_eq!(rows[0].output_tokens, 6);
}
