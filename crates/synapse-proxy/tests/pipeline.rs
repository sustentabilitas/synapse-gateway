//! End-to-end pipeline: context-gated header injection, wrap envelope, 503 on
//! unbound context, and a custom registered transform — all via wiremock.

use std::sync::Arc;

use async_trait::async_trait;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use synapse_proxy::build_router_from_config; // added in this task (router.rs)
use synapse_proxy::config::Config;
use synapse_proxy::context::ResolvedContext;
use synapse_proxy::transform::{ProxyRequest, RequestTransform, TransformError};
use synapse_proxy::ProxyBuilder;

fn request(uri: &str, body: &str) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .body(axum::body::Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn injects_context_header_and_forwards() {
    std::env::set_var("PIPELINE_ORG", "acme");
    let up = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat"))
        .and(header("x-tenant-id", "acme"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&up)
        .await;
    let cfg = Config::from_toml_str(&format!(
        r#"
        [context]
        env = {{ org = "PIPELINE_ORG" }}
        [[routes]]
        path_prefix = "/llm"
        upstream = "{}"
        strip_prefix = true
        require_context = ["org"]
        request_steps = [ {{ inject = {{ header = "x-tenant-id", from_context = "org" }} }} ]
    "#,
        up.uri()
    ))
    .unwrap();
    let app = build_router_from_config(ProxyBuilder::from_config(cfg)).unwrap();
    let resp = app.oneshot(request("/llm/chat", "{}")).await.unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn unbound_required_context_returns_503() {
    let cfg = Config::from_toml_str(
        r#"
        [[routes]]
        path_prefix = "/llm"
        upstream = "http://127.0.0.1:1"
        require_context = ["org"]
    "#,
    )
    .unwrap();
    let app = build_router_from_config(ProxyBuilder::from_config(cfg)).unwrap();
    let resp = app.oneshot(request("/llm/x", "{}")).await.unwrap();
    assert_eq!(resp.status(), 503);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["error"], "request_failed");
    assert_eq!(body["detail"], "context not bound");
}

struct AddMarker;
#[async_trait]
impl RequestTransform for AddMarker {
    async fn apply(
        &self,
        _c: &ResolvedContext,
        req: &mut ProxyRequest,
    ) -> Result<(), TransformError> {
        req.set_header("x-custom", "marker");
        Ok(())
    }
}

#[tokio::test]
async fn custom_registered_transform_runs() {
    let up = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/x"))
        .and(header("x-custom", "marker"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&up)
        .await;
    let cfg = Config::from_toml_str(&format!(
        r#"
        [[routes]]
        path_prefix = "/c"
        upstream = "{}"
        strip_prefix = true
        request_steps = [ {{ transform = "marker" }} ]
    "#,
        up.uri()
    ))
    .unwrap();
    let builder = ProxyBuilder::from_config(cfg).request_transform("marker", Arc::new(AddMarker));
    let app = build_router_from_config(builder).unwrap();
    let resp = app.oneshot(request("/c/x", "{}")).await.unwrap();
    assert_eq!(resp.status(), 204);
}

struct AlwaysReject;
#[async_trait::async_trait]
impl synapse_proxy::transform::RequestTransform for AlwaysReject {
    async fn apply(
        &self,
        _c: &synapse_proxy::context::ResolvedContext,
        _r: &mut synapse_proxy::transform::ProxyRequest,
    ) -> Result<(), synapse_proxy::transform::TransformError> {
        Err(synapse_proxy::transform::TransformError::Reject {
            status: axum::http::StatusCode::FORBIDDEN,
            error: "blocked".into(),
            detail: "nope".into(),
        })
    }
}

#[tokio::test]
async fn request_transform_reject_short_circuits() {
    let cfg = synapse_proxy::config::Config::from_toml_str(
        r#"
        [[routes]]
        path_prefix = "/r"
        upstream = "http://127.0.0.1:1"
        request_steps = [ { transform = "reject" } ]
    "#,
    )
    .unwrap();
    let builder = synapse_proxy::ProxyBuilder::from_config(cfg)
        .request_transform("reject", std::sync::Arc::new(AlwaysReject));
    let app = synapse_proxy::build_router_from_config(builder).unwrap();
    let resp = tower::ServiceExt::oneshot(
        app,
        axum::http::Request::builder()
            .method("POST")
            .uri("/r/x")
            .body(axum::body::Body::from("{}"))
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), 403);
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"], "blocked");
    assert_eq!(body["detail"], "nope");
}
