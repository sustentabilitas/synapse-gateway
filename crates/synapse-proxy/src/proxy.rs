//! Reverse-proxy pipeline: match (path_prefix + methods) → resolve context
//! (gate require_context) → request transforms → forward (stream/hop-by-hop/cap)
//! → response transforms → stream/replace.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::builder::CompiledRoute;
use crate::context::ContextStore;
use crate::transform::{ProxyRequest, ProxyResponse, TransformError};

const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

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
    HOP_BY_HOP.contains(&name.to_ascii_lowercase().as_str())
}

#[derive(Clone)]
pub struct AppState {
    pub routes: Arc<Vec<CompiledRoute>>,
    pub context: Arc<ContextStore>,
    pub client: reqwest::Client,
    pub shutting_down: Arc<AtomicBool>,
    pub metrics: Arc<crate::metrics::Metrics>,
}

/// Longest `path_prefix` match, then narrow by `methods` (empty = any).
pub fn match_route<'a>(
    routes: &'a [CompiledRoute],
    path: &str,
    method: &str,
) -> Option<&'a CompiledRoute> {
    routes
        .iter()
        .filter(|r| path.starts_with(&r.path_prefix))
        .filter(|r| r.methods.is_empty() || r.methods.iter().any(|m| m == method))
        .max_by_key(|r| r.path_prefix.len())
}

fn err(status: StatusCode, error: &str, detail: String) -> Response {
    (
        status,
        Json(serde_json::json!({ "error": error, "detail": detail })),
    )
        .into_response()
}

fn reject_response(e: TransformError) -> Response {
    match e {
        TransformError::Reject {
            status,
            error,
            detail,
        } => err(status, &error, detail),
        TransformError::Internal(m) => err(StatusCode::INTERNAL_SERVER_ERROR, "transform_error", m),
    }
}

fn reject_status(e: &TransformError) -> u16 {
    match e {
        TransformError::Reject { status, .. } => status.as_u16(),
        TransformError::Internal(_) => 500,
    }
}

pub async fn handler(State(state): State<AppState>, req: Request) -> Response {
    let started = std::time::Instant::now();
    let (parts, body) = req.into_parts();
    let path = parts.uri.path().to_string();
    let method = parts.method.as_str().to_string();

    let Some(route) = match_route(&state.routes, &path, &method) else {
        let secs = started.elapsed().as_secs_f64();
        state.metrics.record("none", &method, 404, "no_route", secs);
        return err(
            StatusCode::NOT_FOUND,
            "no_route",
            format!("no route matches '{method} {path}'"),
        );
    };

    let route_label = route.name.clone();

    let ctx = state.context.resolve();
    for key in &route.require_context {
        if !ctx.contains(key) {
            let secs = started.elapsed().as_secs_f64();
            state
                .metrics
                .record(&route_label, &method, 503, "context_unbound", secs);
            return err(
                StatusCode::SERVICE_UNAVAILABLE,
                "request_failed",
                "context not bound".into(),
            );
        }
    }

    let bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b.to_vec(),
        Err(e) => {
            return err(
                StatusCode::PAYLOAD_TOO_LARGE,
                "body_too_large",
                e.to_string(),
            )
        }
    };

    // Inbound headers minus hop-by-hop become the transform-visible header set.
    let mut headers = HeaderMap::new();
    for (n, v) in parts.headers.iter() {
        if !is_hop_by_hop(n.as_str()) {
            headers.insert(n.clone(), v.clone());
        }
    }
    let query = parts.uri.query().map(str::to_string);
    let mut preq = ProxyRequest::from_parts(
        parts.method.clone(),
        path.clone(),
        query.clone(),
        headers,
        bytes,
    );

    for t in &route.request {
        if let Err(e) = t.apply(&ctx, &mut preq).await {
            let secs = started.elapsed().as_secs_f64();
            state.metrics.record(
                &route_label,
                &method,
                reject_status(&e),
                "transform_rejected",
                secs,
            );
            state.metrics.transform_error(&route_label, "request");
            return reject_response(e);
        }
    }

    // Build the upstream URL (prefix strip + query).
    let rest = if route.strip_prefix {
        preq.path
            .strip_prefix(&route.path_prefix)
            .unwrap_or(&preq.path)
    } else {
        &preq.path
    };
    let base = route.upstream.trim_end_matches('/');
    let mut url = format!("{base}{rest}");
    if let Some(q) = preq.query.as_deref().filter(|q| !q.is_empty()) {
        url.push('?');
        url.push_str(q);
    }

    let method_val = preq.method.clone();
    let out_headers = preq.headers.clone();
    let out_body = preq.into_body_bytes();

    let upstream = state
        .client
        .request(method_val, &url)
        .headers(out_headers)
        .body(out_body)
        .send()
        .await;
    let resp = match upstream {
        Ok(r) => r,
        Err(e) => {
            let secs = started.elapsed().as_secs_f64();
            state
                .metrics
                .record(&route_label, &method, 502, "upstream_error", secs);
            state.metrics.upstream_error(&route_label, "send");
            return err(StatusCode::BAD_GATEWAY, "request_failed", e.to_string());
        }
    };

    // Response transforms (status/headers/replacement only in v1).
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut resp_headers = HeaderMap::new();
    for (n, v) in resp.headers().iter() {
        if !is_hop_by_hop(n.as_str()) {
            resp_headers.insert(n.clone(), v.clone());
        }
    }
    let mut presp = ProxyResponse::new(status, resp_headers);
    for t in &route.response {
        if let Err(e) = t.apply(&ctx, &mut presp).await {
            let secs = started.elapsed().as_secs_f64();
            state.metrics.record(
                &route_label,
                &method,
                reject_status(&e),
                "transform_rejected",
                secs,
            );
            state.metrics.transform_error(&route_label, "response");
            return reject_response(e);
        }
    }

    let final_status = presp.status.as_u16();
    let secs = started.elapsed().as_secs_f64();
    state
        .metrics
        .record(&route_label, &method, final_status, "forwarded", secs);

    if let Some(replacement) = presp.replacement() {
        return (presp.status, Json(replacement.clone())).into_response();
    }

    // No body replacement → stream the upstream body unchanged.
    let mut builder = Response::builder().status(presp.status);
    for (n, v) in presp.headers.iter() {
        builder = builder.header(n, v);
    }
    builder
        .body(Body::from_stream(resp.bytes_stream()))
        .unwrap_or_else(|e| err(StatusCode::BAD_GATEWAY, "request_failed", e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::CompiledRoute;

    fn route(prefix: &str, methods: &[&str]) -> CompiledRoute {
        CompiledRoute {
            name: prefix.into(),
            path_prefix: prefix.into(),
            upstream: "http://u".into(),
            strip_prefix: false,
            methods: methods.iter().map(|s| s.to_string()).collect(),
            require_context: vec![],
            request: vec![],
            response: vec![],
        }
    }

    #[test]
    fn matches_longest_prefix_and_method() {
        let routes = vec![route("/v1", &[]), route("/v1/llm", &["POST"])];
        assert_eq!(
            match_route(&routes, "/v1/llm/x", "POST")
                .unwrap()
                .path_prefix,
            "/v1/llm"
        );
        // method mismatch on the longer route → falls back to the any-method route
        assert_eq!(
            match_route(&routes, "/v1/llm/x", "GET")
                .unwrap()
                .path_prefix,
            "/v1"
        );
        assert!(match_route(&routes, "/nope", "GET").is_none());
    }
}
