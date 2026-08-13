//! Upstream send-retry behaviour: connect errors are retried with backoff —
//! even for POST, since a connect failure never put the request on the wire
//! (the MCS endpoint-churn case) — and SYNAPSE_PROXY_UPSTREAM_SEND_RETRIES=0
//! restores the one-shot 502 request_failed.
//!
//! Env vars are process-global, so the tests in this file serialize on a
//! mutex; other test files run as separate binaries and are unaffected.

use http_body_util::BodyExt;
use tower::ServiceExt; // oneshot

use synapse_proxy::build_router_from_config;
use synapse_proxy::config::Config;
use synapse_proxy::ProxyBuilder;

static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn post(uri: &str) -> axum::http::Request<axum::body::Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .body(axum::body::Body::from("{}"))
        .unwrap()
}

fn config_for(upstream: &str) -> Config {
    Config::from_toml_str(&format!(
        r#"
        [[routes]]
        path_prefix = "/v1"
        upstream = "{upstream}"
        strip_prefix = true
    "#
    ))
    .unwrap()
}

/// Bind :0 to reserve a free port, then release it so the test can control
/// when a real listener appears there.
fn reserve_addr() -> std::net::SocketAddr {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

#[tokio::test]
async fn post_rides_through_connect_refused_until_upstream_recovers() {
    let _g = ENV_LOCK.lock().await;
    std::env::set_var("SYNAPSE_PROXY_UPSTREAM_SEND_RETRIES", "4");
    std::env::set_var("SYNAPSE_PROXY_UPSTREAM_RETRY_BACKOFF_MS", "200");

    // No listener yet: the first attempt(s) get connection refused. The
    // upstream comes up mid-backoff (retry schedule: ~0/200/600/1400ms).
    let addr = reserve_addr();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let app = axum::Router::new().route("/echo", axum::routing::post(|| async { "ok" }));
        axum::serve(listener, app).await.unwrap();
    });

    let cfg = config_for(&format!("http://{addr}"));
    let app = build_router_from_config(ProxyBuilder::from_config(cfg)).unwrap();
    let resp = app.oneshot(post("/v1/echo")).await.unwrap();

    std::env::remove_var("SYNAPSE_PROXY_UPSTREAM_SEND_RETRIES");
    std::env::remove_var("SYNAPSE_PROXY_UPSTREAM_RETRY_BACKOFF_MS");

    assert_eq!(resp.status(), 200);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn zero_retries_returns_request_failed_immediately() {
    let _g = ENV_LOCK.lock().await;
    std::env::set_var("SYNAPSE_PROXY_UPSTREAM_SEND_RETRIES", "0");

    let addr = reserve_addr();
    let cfg = config_for(&format!("http://{addr}"));
    let app = build_router_from_config(ProxyBuilder::from_config(cfg)).unwrap();
    let started = std::time::Instant::now();
    let resp = app.oneshot(post("/v1/echo")).await.unwrap();

    std::env::remove_var("SYNAPSE_PROXY_UPSTREAM_SEND_RETRIES");

    assert_eq!(resp.status(), 502);
    let body: serde_json::Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["error"], "request_failed");
    // No backoff sleeps happened.
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}
