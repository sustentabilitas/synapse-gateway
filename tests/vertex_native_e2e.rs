//! Live Vertex native E2E tests (real GCP + running gateway).
//!
//! Ignored by default so `cargo test` stays offline-friendly. Run via docker-compose:
//!   docker compose -f docker-compose.e2e.yml up --build --abort-on-container-exit
//!
//! Or locally with a running gateway and ADC:
//!   GATEWAY_URL=http://127.0.0.1:8080 cargo test --test vertex_native_e2e -- --ignored --nocapture

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use synapse::providers::vertex_auth::VertexAuth;

const ROUTE_ALIAS: &str = "vertex-native";

fn vertex_env() -> (String, String, String) {
    let project = std::env::var("VERTEX_PROJECT_ID")
        .or_else(|_| std::env::var("VERTEX_PROJECT"))
        .expect("VERTEX_PROJECT_ID or VERTEX_PROJECT must be set for E2E");
    let location = std::env::var("VERTEX_LOCATION").unwrap_or_else(|_| "us-central1".into());
    let model = std::env::var("VERTEX_E2E_MODEL").unwrap_or_else(|_| "gemini-2.5-flash".into());
    (project, location, model)
}

fn vertex_host(location: &str) -> String {
    if location == "global" {
        "https://aiplatform.googleapis.com".into()
    } else {
        format!("https://{location}-aiplatform.googleapis.com")
    }
}

fn gateway_url() -> String {
    std::env::var("GATEWAY_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into())
}

async fn bearer_token() -> String {
    Arc::new(VertexAuth::from_adc())
        .token()
        .await
        .expect("ADC token for Vertex E2E")
}

fn padded_cache_context(core: &str) -> String {
    // Vertex context cache requires >= 1024 tokens; repeat a stable paragraph until we exceed it.
    const PARA: &str = "Synapse gateway end-to-end context caching validation paragraph. \
        It documents routing through the native Vertex lane, cachedContents resources, \
        and OpenAI-compatible chat completions over HTTP. ";
    let mut out = String::from(core);
    out.push_str("\n\nBackground:\n");
    while out.len() < 8_000 {
        out.push_str(PARA);
    }
    out
}

async fn create_cached_content(
    project: &str,
    location: &str,
    model: &str,
    context: &str,
) -> String {
    let token = bearer_token().await;
    let url = format!(
        "{}/v1/projects/{}/locations/{}/cachedContents",
        vertex_host(location),
        project,
        location
    );
    let model_resource = format!(
        "projects/{}/locations/{}/publishers/google/models/{}",
        project, location, model
    );
    let body = json!({
        "model": model_resource,
        "contents": [{
            "role": "user",
            "parts": [{ "text": padded_cache_context(context) }]
        }],
        "displayName": format!("synapse-e2e-{}", uuid::Uuid::new_v4()),
        "ttl": "300s"
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .unwrap();
    let resp = client
        .post(&url)
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .expect("create cachedContents request");
    let status = resp.status();
    let value: Value = resp.json().await.expect("cachedContents response json");
    assert!(
        status.is_success(),
        "create cachedContents failed ({status}): {value}"
    );
    value["name"]
        .as_str()
        .expect("cachedContents.name")
        .to_string()
}

async fn chat_completion(body: Value) -> Value {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .unwrap();
    let url = format!("{}/v1/chat/completions", gateway_url());
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .expect("chat/completions request");
    let status = resp.status();
    let value: Value = resp.json().await.expect("chat/completions response json");
    assert!(
        status.is_success(),
        "chat/completions failed ({status}): {value}"
    );
    value
}

fn assistant_text(completion: &Value) -> String {
    completion["choices"][0]["message"]["content"]
        .as_str()
        .expect("assistant message content")
        .to_string()
}

fn parse_json_payload(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| {
        // Some models occasionally wrap JSON in a fenced block; peel one layer.
        let trimmed = text.trim();
        if let Some(inner) = trimmed
            .strip_prefix("```json")
            .and_then(|s| s.strip_suffix("```"))
        {
            return serde_json::from_str(inner.trim())
                .unwrap_or_else(|e| panic!("expected JSON in fence, got {inner:?}: {e}"));
        }
        panic!("expected JSON object in assistant content, got {text:?}");
    })
}

/// Native lane via `cached_content`: plain text completion (no `response_schema`).
#[tokio::test]
#[ignore = "live Vertex E2E — run via docker-compose.e2e.yml or with --ignored"]
async fn e2e_native_cached_content_plain_text() {
    let (project, location, model) = vertex_env();
    let secret = format!(
        "SYNAPSE_E2E_{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let context = format!(
        "Reference document for E2E tests. The secret codeword is {secret}. \
         When asked for the codeword, reply with that exact token and nothing else."
    );
    let cache = create_cached_content(&project, &location, &model, &context).await;

    let completion = chat_completion(json!({
        "model": ROUTE_ALIAS,
        "messages": [{
            "role": "user",
            "content": "Using only the cached reference, what is the secret codeword?"
        }],
        "vertex": {
            "cached_content": cache
        }
    }))
    .await;

    let text = assistant_text(&completion);
    assert!(
        text.contains(&secret),
        "expected codeword {secret} in response, got: {text}"
    );
    assert!(
        completion["usage"]["prompt_tokens"].as_u64().unwrap_or(0) > 0,
        "expected usage metadata: {completion}"
    );
}

/// Native lane with `response_schema` constrained JSON (no cache).
#[tokio::test]
#[ignore = "live Vertex E2E — run via docker-compose.e2e.yml or with --ignored"]
async fn e2e_native_structured_output() {
    let _ = vertex_env();

    let completion = chat_completion(json!({
        "model": ROUTE_ALIAS,
        "messages": [{
            "role": "user",
            "content": "Return a JSON object with field answer set to the word hello."
        }],
        "vertex": {
            "response_schema": {
                "type": "object",
                "properties": {
                    "answer": { "type": "string" }
                },
                "required": ["answer"]
            }
        }
    }))
    .await;

    let parsed = parse_json_payload(&assistant_text(&completion));
    assert_eq!(
        parsed["answer"].as_str(),
        Some("hello"),
        "structured payload: {parsed}"
    );
}

/// Cached context plus structured output on the same native-lane request.
#[tokio::test]
#[ignore = "live Vertex E2E — run via docker-compose.e2e.yml or with --ignored"]
async fn e2e_native_cached_content_with_structured_output() {
    let (project, location, model) = vertex_env();
    let city = "Lisbon";
    let context = format!(
        "City facts for tests: the featured city is {city}. \
         All answers about the featured city must use this name exactly."
    );
    let cache = create_cached_content(&project, &location, &model, &context).await;

    let completion = chat_completion(json!({
        "model": ROUTE_ALIAS,
        "messages": [{
            "role": "user",
            "content": "From the cached reference, what is the featured city?"
        }],
        "vertex": {
            "cached_content": cache,
            "response_schema": {
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                },
                "required": ["city"]
            }
        }
    }))
    .await;

    let parsed = parse_json_payload(&assistant_text(&completion));
    assert_eq!(
        parsed["city"].as_str(),
        Some(city),
        "structured cached response: {parsed}"
    );
}
