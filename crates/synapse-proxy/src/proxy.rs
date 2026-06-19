//! Reverse-proxy forwarding: match a route by longest path prefix, build the
//! upstream URL, copy the request (minus hop-by-hop headers) plus injected
//! headers, and stream the response back.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::config::Route;

/// Maximum request body the proxy will buffer before forwarding (64 MiB).
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Headers that must not be forwarded verbatim across a proxy hop.
pub const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-connection",
    "transfer-encoding",
    "upgrade",
    "te",
    "trailer",
    "host",
];

pub fn is_hop_by_hop(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    HOP_BY_HOP.contains(&lower.as_str())
}

/// The route whose `path_prefix` matches `path` and is longest, or `None`.
pub fn match_route<'a>(routes: &'a [Route], path: &str) -> Option<&'a Route> {
    routes
        .iter()
        .filter(|r| path.starts_with(&r.path_prefix))
        .max_by_key(|r| r.path_prefix.len())
}

/// Build the upstream URL: `{upstream}` + path (prefix stripped when configured)
/// + original query string.
pub fn upstream_url(route: &Route, path: &str, query: Option<&str>) -> String {
    let rest = if route.strip_prefix {
        path.strip_prefix(&route.path_prefix).unwrap_or(path)
    } else {
        path
    };
    let base = route.upstream.trim_end_matches('/');
    let mut url = format!("{base}{rest}");
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        url.push('?');
        url.push_str(q);
    }
    url
}

/// Shared server state: the configured routes, a reusable HTTP client, and the
/// graceful-shutdown flag (read by the readiness probe).
#[derive(Clone)]
pub struct AppState {
    pub routes: Arc<Vec<Route>>,
    pub client: reqwest::Client,
    pub shutting_down: Arc<AtomicBool>,
}

fn error_json(status: StatusCode, error: &str, detail: String) -> Response {
    (
        status,
        Json(serde_json::json!({ "error": error, "detail": detail })),
    )
        .into_response()
}

/// Forward the request to the matched upstream and stream the response back.
pub async fn handler(State(state): State<AppState>, req: Request) -> Response {
    let (parts, body) = req.into_parts();
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().map(str::to_string);

    let Some(route) = match_route(&state.routes, &path) else {
        return error_json(
            StatusCode::NOT_FOUND,
            "no_route",
            format!("no route matches '{path}'"),
        );
    };
    let url = upstream_url(route, &path, query.as_deref());

    let bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(e) => {
            return error_json(
                StatusCode::PAYLOAD_TOO_LARGE,
                "body_too_large",
                e.to_string(),
            )
        }
    };

    // Forward inbound headers minus hop-by-hop, then apply injected headers.
    let mut headers = HeaderMap::new();
    for (name, value) in parts.headers.iter() {
        if !is_hop_by_hop(name.as_str()) {
            headers.insert(name.clone(), value.clone());
        }
    }
    for (k, v) in &route.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(k.as_str()),
            HeaderValue::try_from(v.as_str()),
        ) {
            headers.insert(name, value);
        }
    }

    let upstream = state
        .client
        .request(parts.method, &url)
        .headers(headers)
        .body(bytes)
        .send()
        .await;

    let resp = match upstream {
        Ok(r) => r,
        Err(e) => return error_json(StatusCode::BAD_GATEWAY, "request_failed", e.to_string()),
    };

    let status = resp.status();
    let mut builder = Response::builder().status(status);
    for (name, value) in resp.headers().iter() {
        if !is_hop_by_hop(name.as_str()) {
            builder = builder.header(name, value);
        }
    }
    let stream = resp.bytes_stream();
    builder
        .body(Body::from_stream(stream))
        .unwrap_or_else(|e| error_json(StatusCode::BAD_GATEWAY, "request_failed", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn route(prefix: &str, upstream: &str, strip: bool) -> Route {
        Route {
            path_prefix: prefix.to_string(),
            upstream: upstream.to_string(),
            strip_prefix: strip,
            headers: HashMap::new(),
        }
    }

    #[test]
    fn matches_longest_prefix() {
        let routes = vec![
            route("/v1", "http://a", false),
            route("/v1/llm", "http://b", false),
        ];
        assert_eq!(
            match_route(&routes, "/v1/llm/chat").unwrap().upstream,
            "http://b"
        );
        assert_eq!(
            match_route(&routes, "/v1/other").unwrap().upstream,
            "http://a"
        );
        assert!(match_route(&routes, "/nope").is_none());
    }

    #[test]
    fn builds_url_with_strip_and_query() {
        let r = route("/v1/llm", "http://b:8080/", true);
        assert_eq!(
            upstream_url(&r, "/v1/llm/chat", Some("stream=true")),
            "http://b:8080/chat?stream=true"
        );
    }

    #[test]
    fn builds_url_without_strip_and_no_query() {
        let r = route("/v1/llm", "http://b:8080", false);
        assert_eq!(
            upstream_url(&r, "/v1/llm/chat", None),
            "http://b:8080/v1/llm/chat"
        );
        assert_eq!(
            upstream_url(&r, "/v1/llm/chat", Some("")),
            "http://b:8080/v1/llm/chat"
        );
    }

    #[test]
    fn hop_by_hop_is_case_insensitive() {
        assert!(is_hop_by_hop("Connection"));
        assert!(is_hop_by_hop("HOST"));
        assert!(!is_hop_by_hop("x-forwarded-by"));
    }
}
