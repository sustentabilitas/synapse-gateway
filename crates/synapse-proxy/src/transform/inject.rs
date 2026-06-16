//! `inject` built-in: set a header or a dotted body path from a context key or
//! a constant. Caller-supplied values at the target are overwritten.

use async_trait::async_trait;
use serde_json::Value;

use crate::config::InjectSpec;
use crate::context::ResolvedContext;

use super::{ProxyRequest, RequestTransform, TransformError};

#[derive(Clone)]
enum Target {
    Header(String),
    Body(String),
}

#[derive(Clone)]
enum Source {
    Context(String),
    Const(Value),
}

#[derive(Clone)]
pub struct Inject {
    target: Target,
    source: Source,
}

impl Inject {
    /// Validate exactly one target and one source.
    pub fn from_spec(spec: &InjectSpec) -> anyhow::Result<Self> {
        let target = match (&spec.header, &spec.body) {
            (Some(h), None) => Target::Header(h.clone()),
            (None, Some(b)) => Target::Body(b.clone()),
            _ => anyhow::bail!("inject requires exactly one of `header` or `body`"),
        };
        let source = match (&spec.from_context, &spec.constant) {
            (Some(k), None) => Source::Context(k.clone()),
            (None, Some(v)) => Source::Const(v.clone()),
            _ => anyhow::bail!("inject requires exactly one of `from_context` or `const`"),
        };
        Ok(Self { target, source })
    }

    fn resolve_value(&self, ctx: &ResolvedContext) -> Option<Value> {
        match &self.source {
            Source::Const(v) => Some(v.clone()),
            Source::Context(k) => ctx.get(k).map(|s| Value::String(s.to_string())),
        }
    }
}

/// Set a dotted path (`a.b.c`) in `root`, creating intermediate objects.
pub fn set_body_path(root: &mut Value, path: &str, value: Value) {
    let mut cur = root;
    let parts: Vec<&str> = path.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        if !cur.is_object() {
            *cur = Value::Object(Default::default());
        }
        let Some(obj) = cur.as_object_mut() else {
            return;
        };
        if i == parts.len() - 1 {
            obj.insert((*part).to_string(), value);
            return;
        }
        cur = obj
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
}

#[async_trait]
impl RequestTransform for Inject {
    async fn apply(
        &self,
        ctx: &ResolvedContext,
        req: &mut ProxyRequest,
    ) -> Result<(), TransformError> {
        let value = self.resolve_value(ctx);
        match &self.target {
            Target::Header(name) => match value {
                Some(v) => {
                    let s = match &v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    req.set_header(name, &s);
                }
                None => req.remove_header(name), // fail-safe: strip any caller-supplied value
            },
            Target::Body(path) => {
                if let Some(v) = value {
                    let body = req.body_json_mut()?;
                    set_body_path(body, path, v);
                }
                // Body target with absent context: left as-is (require_context is the gate).
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::ProxyRequest;
    use axum::http::{HeaderMap, Method};
    use std::collections::HashMap;

    fn ctx() -> ResolvedContext {
        // build via ContextStore to avoid exposing internals
        let s = crate::context::ContextStore::new(HashMap::from([("org".into(), "acme".into())]));
        s.resolve()
    }
    fn req(body: &[u8]) -> ProxyRequest {
        ProxyRequest::from_parts(
            Method::POST,
            "/x".into(),
            None,
            HeaderMap::new(),
            body.to_vec(),
        )
    }

    #[tokio::test]
    async fn injects_context_header_overwriting() {
        let inj = Inject::from_spec(&InjectSpec {
            header: Some("X-Tenant-Id".into()),
            body: None,
            from_context: Some("org".into()),
            constant: None,
        })
        .unwrap();
        let mut r = req(b"");
        r.set_header("x-tenant-id", "attacker");
        inj.apply(&ctx(), &mut r).await.unwrap();
        assert_eq!(r.headers.get("X-Tenant-Id").unwrap(), "acme"); // overwritten
    }

    #[tokio::test]
    async fn injects_constant_into_nested_body_path() {
        let inj = Inject::from_spec(&InjectSpec {
            header: None,
            body: Some("params.context.user".into()),
            from_context: None,
            constant: Some(serde_json::json!("_default")),
        })
        .unwrap();
        let mut r = req(b"{\"params\":{}}");
        inj.apply(&ctx(), &mut r).await.unwrap();
        let v: Value = serde_json::from_slice(&r.into_body_bytes()).unwrap();
        assert_eq!(v["params"]["context"]["user"], "_default");
    }

    #[test]
    fn from_spec_rejects_ambiguous_target() {
        let bad = InjectSpec {
            header: Some("h".into()),
            body: Some("b".into()),
            from_context: Some("k".into()),
            constant: None,
        };
        assert!(Inject::from_spec(&bad).is_err());
    }

    #[test]
    fn set_body_path_single_segment() {
        let mut v = serde_json::json!({});
        set_body_path(&mut v, "org", serde_json::json!("acme"));
        assert_eq!(v["org"], "acme");
    }

    #[test]
    fn set_body_path_overwrites_non_object_intermediate() {
        let mut v = serde_json::json!({ "a": "scalar" });
        set_body_path(&mut v, "a.b", serde_json::json!(1));
        assert_eq!(v["a"]["b"], 1); // scalar intermediate replaced by an object
    }

    #[tokio::test]
    async fn absent_context_header_is_stripped_not_passed_through() {
        // context has no "user" key
        let s = crate::context::ContextStore::new(std::collections::HashMap::new());
        let ctx = s.resolve();
        let inj = Inject::from_spec(&InjectSpec {
            header: Some("x-user-id".into()),
            body: None,
            from_context: Some("user".into()),
            constant: None,
        })
        .unwrap();
        let mut r = req(b""); // helper in this test module
        r.set_header("x-user-id", "attacker"); // caller-supplied identity
        inj.apply(&ctx, &mut r).await.unwrap();
        assert!(
            r.headers.get("x-user-id").is_none(),
            "caller identity must be stripped when context absent"
        );
    }
}
