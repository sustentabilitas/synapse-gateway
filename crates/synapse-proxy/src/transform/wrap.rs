//! `wrap` built-in: nest the incoming JSON body under a key, then inject sibling
//! fields. Reproduces the integration `/call` envelope:
//! `{ <under>: <original body>, org: …, workspace: … }`.

use async_trait::async_trait;
use serde_json::Value;

use crate::config::WrapSpec;
use crate::context::ResolvedContext;

use super::inject::{set_body_path, Inject};
use super::{ProxyRequest, RequestTransform, TransformError};

pub struct Wrap {
    under: String,
    siblings: Vec<Inject>,
}

impl Wrap {
    pub fn from_spec(spec: &WrapSpec) -> anyhow::Result<Self> {
        let siblings = spec
            .inject
            .iter()
            .map(Inject::from_spec)
            .collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self {
            under: spec.under.clone(),
            siblings,
        })
    }
}

#[async_trait]
impl RequestTransform for Wrap {
    async fn apply(
        &self,
        ctx: &ResolvedContext,
        req: &mut ProxyRequest,
    ) -> Result<(), TransformError> {
        // Take the existing body, nest it under `under`, then inject siblings.
        let original = req.body_json_mut()?.take(); // serde_json::Value::take leaves Null
        let mut envelope = Value::Object(Default::default());
        set_body_path(&mut envelope, &self.under, original);
        *req.body_json_mut()? = envelope;
        for inj in &self.siblings {
            inj.apply(ctx, req).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InjectSpec;
    use crate::transform::ProxyRequest;
    use axum::http::{HeaderMap, Method};
    use std::collections::HashMap;

    fn ctx() -> ResolvedContext {
        crate::context::ContextStore::new(HashMap::from([("org".into(), "acme".into())])).resolve()
    }

    #[tokio::test]
    async fn wraps_body_and_injects_sibling() {
        let wrap = Wrap::from_spec(&WrapSpec {
            under: "request".into(),
            inject: vec![InjectSpec {
                header: None,
                body: Some("org".into()),
                from_context: Some("org".into()),
                constant: None,
            }],
        })
        .unwrap();
        let mut r = ProxyRequest::from_parts(
            Method::POST,
            "/x".into(),
            None,
            HeaderMap::new(),
            b"{\"method\":\"GET\",\"path\":\"/p\"}".to_vec(),
        );
        wrap.apply(&ctx(), &mut r).await.unwrap();
        let v: Value = serde_json::from_slice(&r.into_body_bytes()).unwrap();
        assert_eq!(v["request"]["method"], "GET");
        assert_eq!(v["request"]["path"], "/p");
        assert_eq!(v["org"], "acme");
    }
}
