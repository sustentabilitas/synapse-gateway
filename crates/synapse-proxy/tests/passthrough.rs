//! End-to-end: the proxy forwards to a wiremock upstream with injected headers,
//! streams the response back, and returns the error contract on miss/failure.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use http_body_util::BodyExt;
use tower::ServiceExt; // oneshot
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use synapse_proxy::build_router; // see note below
use synapse_proxy::config::Route;
use synapse_proxy::proxy::AppState;

fn state(routes: Vec<Route>) -> AppState {
    AppState {
        routes: Arc::new(routes),
        client: reqwest::Client::new(),
        shutting_down: Arc::new(AtomicBool::new(false)),
    }
}

fn route(prefix: &str, upstream: &str) -> Route {
    let mut headers = HashMap::new();
    headers.insert("x-forwarded-by".to_string(), "synapse-proxy".to_string());
    Route {
        path_prefix: prefix.to_string(),
        upstream: upstream.to_string(),
        strip_prefix: true,
        headers,
    }
}

#[tokio::test]
async fn forwards_with_injected_header_and_streams_response() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat"))
        .and(header("x-forwarded-by", "synapse-proxy"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hello-from-upstream"))
        .mount(&upstream)
        .await;

    let app = build_router(state(vec![route("/v1/llm", &upstream.uri())]));
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/llm/chat")
                .body(axum::body::Body::from("ping"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello-from-upstream");
}

#[tokio::test]
async fn unmatched_path_returns_404_no_route() {
    let app = build_router(state(vec![route("/v1/llm", "http://127.0.0.1:1")]));
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/nope")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "no_route");
}

#[tokio::test]
async fn unreachable_upstream_returns_502_request_failed() {
    let app = build_router(state(vec![route("/v1/llm", "http://127.0.0.1:1")]));
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/v1/llm/x")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "request_failed");
}
