#![cfg(feature = "server")]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures::StreamExt;
use http_body_util::BodyExt;
use synapse::error::GatewayError;
use synapse::gateway::{Gateway, RequestCtx};
use synapse::ledger::{InMemoryLedger, LedgerHandle, LedgerStore};
use synapse::pricing::PricingTable;
use synapse::providers::vertex_auth::VertexAuth;
use synapse::providers::Catalog;
use synapse::routing::stream::{FinishReason, StreamItem};
use synapse::routing::table::RouteTable;
use synapse::server::router;
use synapse::vertex_native::VertexNativeProvider;
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PROJECT: &str = "p";
const MODEL: &str = "gemini-3-flash";
const ROUTE: &str = "native";
const STREAM_PATH: &str =
    "/v1/projects/p/locations/global/publishers/google/models/gemini-3-flash:streamGenerateContent";

fn vertex_sse_text(content: &str, input: u64, output: u64) -> String {
    let escaped = serde_json::to_string(content).unwrap();
    format!(
        "data: {{\"candidates\":[{{\"content\":{{\"role\":\"model\",\"parts\":[{{\"text\":{escaped}}}]}}}}]}}\n\n\
         data: {{\"candidates\":[{{\"finishReason\":\"STOP\",\"content\":{{\"role\":\"model\",\"parts\":[]}}}}],\"usageMetadata\":{{\"promptTokenCount\":{input},\"candidatesTokenCount\":{output}}}}}\n\n"
    )
}

fn vertex_sse_tool(name: &str, args_json: &str, input: u64, output: u64) -> String {
    format!(
        "data: {{\"candidates\":[{{\"content\":{{\"role\":\"model\",\"parts\":[{{\"functionCall\":{{\"name\":\"{name}\",\"args\":{args_json}}}}}]}}}}]}}\n\n\
         data: {{\"candidates\":[{{\"finishReason\":\"STOP\",\"content\":{{\"role\":\"model\",\"parts\":[]}}}}],\"usageMetadata\":{{\"promptTokenCount\":{input},\"candidatesTokenCount\":{output}}}}}\n\n"
    )
}

async fn mount_vertex_stream(mock: &MockServer, body_needle: Option<&str>, sse: &str) {
    let mut spec = Mock::given(method("POST")).and(path(STREAM_PATH));
    if let Some(needle) = body_needle {
        spec = spec.and(body_string_contains(needle));
    }
    spec.respond_with(
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(sse),
    )
    .mount(mock)
    .await;
}

fn vertex_auth() -> Arc<VertexAuth> {
    Arc::new(VertexAuth::with_fetcher(|| {
        Box::pin(async { Ok(("test-token".into(), Duration::from_secs(3600))) })
    }))
}

fn native_routes() -> RouteTable {
    RouteTable::from_toml_str(&format!(
        r#"[routes."{ROUTE}"]
           legs = [{{ provider = "vertex", model = "{MODEL}" }}]"#,
    ))
    .unwrap()
}

fn native_catalog(routes: &RouteTable) -> Catalog {
    Catalog::build(
        &HashMap::from([("VERTEX_PROJECT_ID".to_string(), PROJECT.to_string())]),
        &routes.referenced_providers(),
        Duration::from_secs(5),
    )
    .unwrap()
}

async fn native_gateway(mock_uri: &str) -> Gateway {
    let routes = native_routes();
    let catalog = native_catalog(&routes);
    let vertex_native = VertexNativeProvider::new(
        vertex_auth(),
        PROJECT.into(),
        "global".into(),
        Duration::from_secs(5),
        Some(mock_uri.to_string()),
    );
    Gateway::builder()
        .routes(routes)
        .catalog(catalog)
        .pricing(PricingTable::default())
        .ledger(LedgerHandle::spawn(
            Arc::new(InMemoryLedger::default()) as Arc<dyn LedgerStore>,
            64,
        ))
        .vertex_native(Some(vertex_native))
        .default_tenant("test")
        .build()
        .unwrap()
}

async fn gateway_without_native_provider() -> Gateway {
    let routes = native_routes();
    let catalog = native_catalog(&routes);
    Gateway::builder()
        .routes(routes)
        .catalog(catalog)
        .pricing(PricingTable::default())
        .ledger(LedgerHandle::spawn(
            Arc::new(InMemoryLedger::default()) as Arc<dyn LedgerStore>,
            16,
        ))
        .default_tenant("test")
        .build()
        .unwrap()
}

// --- Gateway::chat (buffered) -------------------------------------------------

#[tokio::test]
async fn gateway_native_buffered_response_schema() {
    let mock = MockServer::start().await;
    mount_vertex_stream(
        &mock,
        Some("\"responseSchema\""),
        &vertex_sse_text(r#"{"answer":"ok"}"#, 5, 3),
    )
    .await;

    let gw = native_gateway(&mock.uri()).await;
    let req: synapse::routing::request::ChatRequest = serde_json::from_value(serde_json::json!({
        "model": ROUTE,
        "messages": [{"role": "user", "content": "structured please"}],
        "vertex": {
            "response_schema": {
                "type": "object",
                "properties": { "answer": { "type": "string" } },
                "required": ["answer"]
            }
        }
    }))
    .unwrap();

    let c = gw.chat(req, &RequestCtx::default()).await.unwrap();
    assert_eq!(c.content, r#"{"answer":"ok"}"#);
    assert_eq!(c.provider, "vertex");
    assert_eq!(c.model, MODEL);
}

#[tokio::test]
async fn gateway_native_buffered_cached_content() {
    let mock = MockServer::start().await;
    mount_vertex_stream(
        &mock,
        Some("\"cachedContent\":\"cachedContents/e2e-abc\""),
        &vertex_sse_text("cached-ok", 8, 2),
    )
    .await;

    let gw = native_gateway(&mock.uri()).await;
    let req: synapse::routing::request::ChatRequest = serde_json::from_value(serde_json::json!({
        "model": ROUTE,
        "messages": [{"role": "user", "content": "use cache"}],
        "vertex": { "cached_content": "cachedContents/e2e-abc" }
    }))
    .unwrap();

    let c = gw.chat(req, &RequestCtx::default()).await.unwrap();
    assert_eq!(c.content, "cached-ok");
}

#[tokio::test]
async fn gateway_native_buffered_gs_media_uri() {
    let mock = MockServer::start().await;
    mount_vertex_stream(
        &mock,
        Some("gs://bucket/video.mp4"),
        &vertex_sse_text("saw-media", 4, 1),
    )
    .await;

    let gw = native_gateway(&mock.uri()).await;
    let req: synapse::routing::request::ChatRequest = serde_json::from_value(serde_json::json!({
        "model": ROUTE,
        "messages": [{"role": "user", "content": "describe clip"}],
        "vertex": { "media_uris": ["gs://bucket/video.mp4"] }
    }))
    .unwrap();

    let c = gw.chat(req, &RequestCtx::default()).await.unwrap();
    assert_eq!(c.content, "saw-media");
}

#[tokio::test]
async fn gateway_native_buffered_thinking_config_with_schema() {
    let mock = MockServer::start().await;
    mount_vertex_stream(
        &mock,
        Some("\"thinkingConfig\""),
        &vertex_sse_text(r#"{"answer":"low"}"#, 6, 2),
    )
    .await;

    let gw = native_gateway(&mock.uri()).await;
    let req: synapse::routing::request::ChatRequest = serde_json::from_value(serde_json::json!({
        "model": ROUTE,
        "messages": [{"role": "user", "content": "think briefly"}],
        "vertex": {
            "thinking_config": { "thinkingLevel": "low" },
            "response_schema": {
                "type": "object",
                "properties": { "answer": { "type": "string" } },
                "required": ["answer"]
            }
        }
    }))
    .unwrap();

    let c = gw.chat(req, &RequestCtx::default()).await.unwrap();
    assert_eq!(c.content, r#"{"answer":"low"}"#);
}

#[tokio::test]
async fn gateway_native_buffered_tool_calls() {
    let mock = MockServer::start().await;
    mount_vertex_stream(
        &mock,
        Some("\"functionDeclarations\""),
        &vertex_sse_tool("get_weather", r#"{"city":"SF"}"#, 9, 4),
    )
    .await;

    let gw = native_gateway(&mock.uri()).await;
    let req: synapse::routing::request::ChatRequest = serde_json::from_value(serde_json::json!({
        "model": ROUTE,
        "messages": [{"role": "user", "content": "weather?"}],
        "tools": [{
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Lookup",
                "parameters": { "type": "object", "properties": { "city": { "type": "string" } } }
            }
        }],
        "vertex": {
            "response_schema": { "type": "object" }
        }
    }))
    .unwrap();

    let c = gw.chat(req, &RequestCtx::default()).await.unwrap();
    assert_eq!(c.finish_reason, FinishReason::ToolCalls);
    assert_eq!(c.tool_calls.len(), 1);
    assert_eq!(c.tool_calls[0].name, "get_weather");
    assert!(c.tool_calls[0].arguments.contains("SF"));
}

#[tokio::test]
async fn gateway_native_not_configured_is_bad_request() {
    let gw = gateway_without_native_provider().await;
    let req: synapse::routing::request::ChatRequest = serde_json::from_value(serde_json::json!({
        "model": ROUTE,
        "messages": [{"role": "user", "content": "hi"}],
        "vertex": { "response_schema": { "type": "object" } }
    }))
    .unwrap();

    let err = gw.chat(req, &RequestCtx::default()).await.unwrap_err();
    assert!(
        matches!(err, GatewayError::BadRequest(_)),
        "expected BadRequest, got {err:?}"
    );
}

// --- Gateway::chat_stream -----------------------------------------------------

#[tokio::test]
async fn gateway_native_streaming_text_chunks() {
    let mock = MockServer::start().await;
    mount_vertex_stream(&mock, None, &vertex_sse_text("streamed", 2, 1)).await;

    let gw = native_gateway(&mock.uri()).await;
    let req: synapse::routing::request::ChatRequest = serde_json::from_value(serde_json::json!({
        "model": ROUTE,
        "stream": true,
        "messages": [{"role": "user", "content": "hi"}],
        "vertex": { "response_schema": { "type": "object" } }
    }))
    .unwrap();

    let mut stream = gw.chat_stream(req, &RequestCtx::default()).await.unwrap();
    let mut deltas = Vec::new();
    while let Some(item) = stream.next().await {
        match item.unwrap() {
            StreamItem::Delta(t) => deltas.push(t),
            StreamItem::Done { finish_reason, .. } => {
                assert_eq!(finish_reason, FinishReason::Stop);
            }
            _ => {}
        }
    }
    assert_eq!(deltas.join(""), "streamed");
}

#[tokio::test]
async fn gateway_native_streaming_tool_call_chunks() {
    let mock = MockServer::start().await;
    mount_vertex_stream(
        &mock,
        None,
        &vertex_sse_tool("lookup", r#"{"q":"x"}"#, 3, 2),
    )
    .await;

    let gw = native_gateway(&mock.uri()).await;
    let req: synapse::routing::request::ChatRequest = serde_json::from_value(serde_json::json!({
        "model": ROUTE,
        "stream": true,
        "messages": [{"role": "user", "content": "lookup"}],
        "tools": [{
            "type": "function",
            "function": {
                "name": "lookup",
                "parameters": { "type": "object" }
            }
        }],
        "vertex": { "cached_content": "cachedContents/x" }
    }))
    .unwrap();

    let mut stream = gw.chat_stream(req, &RequestCtx::default()).await.unwrap();
    let mut saw_tool = false;
    let mut finish = FinishReason::Stop;
    while let Some(item) = stream.next().await {
        match item.unwrap() {
            StreamItem::ToolCallDelta { name, .. } if name.as_deref() == Some("lookup") => {
                saw_tool = true;
            }
            StreamItem::Done { finish_reason, .. } => finish = finish_reason,
            _ => {}
        }
    }
    assert!(saw_tool, "expected tool-call delta");
    assert_eq!(finish, FinishReason::ToolCalls);
}

// --- HTTP surface -------------------------------------------------------------

#[tokio::test]
async fn http_native_buffered_response_schema() {
    let mock = MockServer::start().await;
    mount_vertex_stream(
        &mock,
        Some("\"responseSchema\""),
        &vertex_sse_text("http-ok", 3, 1),
    )
    .await;

    let gw = Arc::new(native_gateway(&mock.uri()).await);
    let resp = router(gw)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "model": ROUTE,
                        "messages": [{"role": "user", "content": "hi"}],
                        "vertex": {
                            "response_schema": { "type": "object" }
                        }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["object"], "chat.completion");
    assert_eq!(v["choices"][0]["message"]["content"], "http-ok");
}

#[tokio::test]
async fn http_native_streaming_returns_sse_chunks() {
    let mock = MockServer::start().await;
    mount_vertex_stream(&mock, None, &vertex_sse_text("chunked", 2, 2)).await;

    let gw = Arc::new(native_gateway(&mock.uri()).await);
    let resp = router(gw)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "model": ROUTE,
                        "stream": true,
                        "messages": [{"role": "user", "content": "hi"}],
                        "vertex": { "media_uris": ["gs://bucket/v.mp4"] }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.starts_with("text/event-stream"), "content-type: {ct}");
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("chat.completion.chunk"), "body: {text}");
    assert!(text.contains("chunked"), "body: {text}");
    assert!(text.contains("[DONE]"), "body: {text}");
}

#[tokio::test]
async fn http_native_cached_content_records_ledger() {
    let mock = MockServer::start().await;
    mount_vertex_stream(
        &mock,
        Some("cachedContents/ledger"),
        &vertex_sse_text("ok", 4, 2),
    )
    .await;

    let routes = native_routes();
    let catalog = native_catalog(&routes);
    let store = Arc::new(InMemoryLedger::default());
    let vertex_native = VertexNativeProvider::new(
        vertex_auth(),
        PROJECT.into(),
        "global".into(),
        Duration::from_secs(5),
        Some(mock.uri()),
    );
    let gw = Arc::new(
        Gateway::builder()
            .routes(routes)
            .catalog(catalog)
            .pricing(PricingTable::default())
            .ledger(LedgerHandle::spawn(
                store.clone() as Arc<dyn LedgerStore>,
                64,
            ))
            .vertex_native(Some(vertex_native))
            .default_tenant("unattributed")
            .build()
            .unwrap(),
    );

    let resp = router(gw)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .header("x-synapse-tenant", "e2e-tenant")
                .body(Body::from(
                    serde_json::json!({
                        "model": ROUTE,
                        "messages": [{"role": "user", "content": "hi"}],
                        "vertex": { "cached_content": "cachedContents/ledger" }
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    for _ in 0..50 {
        if store.entries.lock().len() == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let entries = store.entries.lock();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tenant, "e2e-tenant");
    assert_eq!(entries[0].lane, "native");
    assert_eq!(entries[0].provider, "vertex");
}
