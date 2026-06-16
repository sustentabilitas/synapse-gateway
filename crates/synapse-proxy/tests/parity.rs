//! Parity: reproduce a broker route end-to-end — context-bound header injection
//! plus an integration `/call` envelope with error_remap — proving the engine
//! replaces broker behavior.

use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use wiremock::matchers::{body_json_string, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use synapse_proxy::build_router_from_config;
use synapse_proxy::config::Config;
use synapse_proxy::ProxyBuilder;

#[tokio::test]
async fn integration_call_envelope_and_error_remap() {
    std::env::set_var("PARITY_ORG", "acme");
    std::env::set_var("PARITY_WS", "ws1");
    let up = MockServer::start().await;
    // Expect the wrapped envelope body.
    Mock::given(method("POST"))
        .and(path("/call-workspace/x"))
        .and(body_json_string(
            r#"{"request":{"method":"GET","path":"/p"},"org":"acme","workspace":"ws1"}"#,
        ))
        .respond_with(ResponseTemplate::new(401).set_body_string("upstream auth error"))
        .mount(&up)
        .await;

    let cfg = Config::from_toml_str(&format!(
        r#"
        [context]
        env = {{ org = "PARITY_ORG", workspace = "PARITY_WS" }}
        [[routes]]
        name = "call"
        path_prefix = "/v1/integrations/call"
        upstream = "{}/call-workspace"
        strip_prefix = true
        require_context = ["org", "workspace"]
        request_steps = [ {{ wrap = {{ under = "request", inject = [
            {{ body = "org", from_context = "org" }},
            {{ body = "workspace", from_context = "workspace" }},
        ] }} }} ]
        response_steps = [ {{ error_remap = {{ when_status = 401, error = "auth_expired" }} }} ]
    "#,
        up.uri()
    ))
    .unwrap();

    let app = build_router_from_config(ProxyBuilder::from_config(cfg)).unwrap();
    let resp = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/v1/integrations/call/x")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(r#"{"method":"GET","path":"/p"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 401); // status kept
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["error"], "auth_expired"); // remapped, no upstream text leaked
}
