//! The transform pipeline: traits a request/response step implements, the
//! mutable request/response views they operate on, and the registry of
//! code-registered custom transforms.

pub mod error_remap;
pub mod inject;
pub mod wrap;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use serde_json::Value;

use crate::context::ResolvedContext;

/// A step's failure: `Reject` short-circuits the pipeline with a contract body.
#[derive(Debug, Clone)]
pub enum TransformError {
    Reject {
        status: StatusCode,
        error: String,
        detail: String,
    },
    Internal(String),
}

/// Mutable view of the request a `RequestTransform` may edit before forwarding.
/// `body` is JSON when present; non-JSON bodies are exposed as raw bytes and are
/// not editable as JSON.
pub struct ProxyRequest {
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: HeaderMap,
    body: Body,
}

enum Body {
    Bytes(Vec<u8>),
    Json(Value),
}

impl ProxyRequest {
    pub fn from_parts(
        method: Method,
        path: String,
        query: Option<String>,
        headers: HeaderMap,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            method,
            path,
            query,
            headers,
            body: Body::Bytes(bytes),
        }
    }

    /// Overwrite a header (skips silently if name/value is invalid).
    pub fn set_header(&mut self, name: &str, value: &str) {
        if let (Ok(n), Ok(v)) = (HeaderName::try_from(name), HeaderValue::try_from(value)) {
            self.headers.insert(n, v);
        }
    }

    /// Remove a header entirely (used to strip a caller value when an injected
    /// context key is absent — fail-safe identity handling).
    pub fn remove_header(&mut self, name: &str) {
        if let Ok(n) = HeaderName::try_from(name) {
            self.headers.remove(n);
        }
    }

    /// Mutable JSON body, parsing the raw bytes on first access. Errors if the
    /// body is not valid JSON (a body transform on a non-JSON body is a reject).
    pub fn body_json_mut(&mut self) -> Result<&mut Value, TransformError> {
        if let Body::Bytes(b) = &self.body {
            let parsed = if b.is_empty() {
                Value::Object(Default::default())
            } else {
                serde_json::from_slice(b).map_err(|e| TransformError::Reject {
                    status: StatusCode::BAD_REQUEST,
                    error: "invalid_body".into(),
                    detail: format!("expected JSON body: {e}"),
                })?
            };
            self.body = Body::Json(parsed);
        }
        match &mut self.body {
            Body::Json(v) => Ok(v),
            Body::Bytes(_) => unreachable!(),
        }
    }

    /// Serialize the (possibly transformed) body back to bytes for forwarding.
    pub fn into_body_bytes(self) -> Vec<u8> {
        match self.body {
            Body::Bytes(b) => b,
            Body::Json(v) => serde_json::to_vec(&v).unwrap_or_default(),
        }
    }

    /// Headers + body ready for the upstream reqwest call.
    ///
    /// When JSON transforms (`inject`/`wrap` on body) re-serialize the payload,
    /// drop stale `Content-Length` / `Transfer-Encoding` from the client so
    /// reqwest emits a length matching the new body (callers like urllib set
    /// Content-Length for the pre-transform size).
    pub fn into_forward_parts(self) -> (HeaderMap, Vec<u8>) {
        let reencoded = matches!(self.body, Body::Json(_));
        let bytes = match self.body {
            Body::Bytes(b) => b,
            Body::Json(v) => serde_json::to_vec(&v).unwrap_or_default(),
        };
        let mut headers = self.headers;
        if reencoded {
            headers.remove("content-length");
            headers.remove("transfer-encoding");
        }
        (headers, bytes)
    }
}

/// Mutable view of the upstream response. v1 transforms touch status/headers and
/// may replace the body with a small JSON value; if no replacement is set the
/// handler streams the upstream body unchanged.
pub struct ProxyResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    replacement: Option<Value>,
}

impl ProxyResponse {
    pub fn new(status: StatusCode, headers: HeaderMap) -> Self {
        Self {
            status,
            headers,
            replacement: None,
        }
    }
    /// Replace the response body with `value` (sent instead of streaming upstream).
    pub fn replace_body(&mut self, value: Value) {
        self.replacement = Some(value);
    }
    pub fn replacement(&self) -> Option<&Value> {
        self.replacement.as_ref()
    }
}

#[async_trait]
pub trait RequestTransform: Send + Sync {
    async fn apply(
        &self,
        ctx: &ResolvedContext,
        req: &mut ProxyRequest,
    ) -> Result<(), TransformError>;
}

#[async_trait]
pub trait ResponseTransform: Send + Sync {
    async fn apply(
        &self,
        ctx: &ResolvedContext,
        resp: &mut ProxyResponse,
    ) -> Result<(), TransformError>;
}

/// Code-registered custom transforms, looked up by `{ transform = "name" }`.
#[derive(Default, Clone)]
pub struct TransformRegistry {
    pub(crate) request: HashMap<String, Arc<dyn RequestTransform>>,
    pub(crate) response: HashMap<String, Arc<dyn ResponseTransform>>,
}

impl TransformRegistry {
    pub fn register_request(&mut self, name: impl Into<String>, t: Arc<dyn RequestTransform>) {
        self.request.insert(name.into(), t);
    }
    pub fn register_response(&mut self, name: impl Into<String>, t: Arc<dyn ResponseTransform>) {
        self.response.insert(name.into(), t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> ProxyRequest {
        ProxyRequest::from_parts(
            Method::POST,
            "/x".into(),
            None,
            HeaderMap::new(),
            b"{\"a\":1}".to_vec(),
        )
    }

    #[test]
    fn set_header_overwrites() {
        let mut r = req();
        r.set_header("x-test", "v1");
        r.set_header("x-test", "v2");
        assert_eq!(r.headers.get("x-test").unwrap(), "v2");
    }

    #[test]
    fn body_json_mut_parses_and_serializes() {
        let mut r = req();
        r.body_json_mut().unwrap()["b"] = serde_json::json!(2);
        let bytes = r.into_body_bytes();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], 2);
    }

    #[test]
    fn into_forward_parts_strips_stale_content_length_after_json_inject() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "10".parse().unwrap());
        headers.insert("transfer-encoding", "chunked".parse().unwrap());
        let mut r = ProxyRequest::from_parts(
            Method::POST,
            "/x".into(),
            None,
            headers,
            br#"{"a":1}"#.to_vec(),
        );
        r.body_json_mut().unwrap()["orgId"] = serde_json::json!("acme");
        let (headers, bytes) = r.into_forward_parts();
        assert!(headers.get("content-length").is_none());
        assert!(headers.get("transfer-encoding").is_none());
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["orgId"], "acme");
        assert!(bytes.len() > 10);
    }

    #[test]
    fn into_forward_parts_keeps_content_length_when_body_unchanged() {
        let mut headers = HeaderMap::new();
        headers.insert("content-length", "7".parse().unwrap());
        let r = ProxyRequest::from_parts(
            Method::POST,
            "/x".into(),
            None,
            headers,
            br#"{"a":1}"#.to_vec(),
        );
        let (headers, bytes) = r.into_forward_parts();
        assert_eq!(headers.get("content-length").unwrap(), "7");
        assert_eq!(bytes, br#"{"a":1}"#);
    }
}
