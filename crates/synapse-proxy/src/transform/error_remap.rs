//! `error_remap` response built-in: when the upstream status equals `when_status`,
//! replace the body with a normalized `{error, detail?}` contract (status kept).

use async_trait::async_trait;
use axum::http::StatusCode;
use serde_json::json;

use crate::config::ErrorRemapSpec;
use crate::context::ResolvedContext;

use super::{ProxyResponse, ResponseTransform, TransformError};

pub struct ErrorRemap {
    when: StatusCode,
    error: String,
    detail: Option<String>,
}

impl ErrorRemap {
    pub fn from_spec(spec: &ErrorRemapSpec) -> anyhow::Result<Self> {
        let when = StatusCode::from_u16(spec.when_status).map_err(|_| {
            anyhow::anyhow!("error_remap: invalid when_status {}", spec.when_status)
        })?;
        Ok(Self {
            when,
            error: spec.error.clone(),
            detail: spec.detail.clone(),
        })
    }
}

#[async_trait]
impl ResponseTransform for ErrorRemap {
    async fn apply(
        &self,
        _ctx: &ResolvedContext,
        resp: &mut ProxyResponse,
    ) -> Result<(), TransformError> {
        if resp.status == self.when {
            let body = match &self.detail {
                Some(d) => json!({ "error": self.error, "detail": d }),
                None => json!({ "error": self.error }),
            };
            resp.replace_body(body);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::ProxyResponse;
    use axum::http::HeaderMap;
    use std::collections::HashMap;

    fn ctx() -> ResolvedContext {
        crate::context::ContextStore::new(HashMap::new()).resolve()
    }

    #[tokio::test]
    async fn remaps_matching_status() {
        let er = ErrorRemap::from_spec(&ErrorRemapSpec {
            when_status: 401,
            error: "auth_expired".into(),
            detail: None,
        })
        .unwrap();
        let mut resp = ProxyResponse::new(StatusCode::UNAUTHORIZED, HeaderMap::new());
        er.apply(&ctx(), &mut resp).await.unwrap();
        assert_eq!(resp.status, StatusCode::UNAUTHORIZED); // status kept
        assert_eq!(resp.replacement().unwrap()["error"], "auth_expired");
    }

    #[tokio::test]
    async fn leaves_non_matching_status() {
        let er = ErrorRemap::from_spec(&ErrorRemapSpec {
            when_status: 401,
            error: "auth_expired".into(),
            detail: None,
        })
        .unwrap();
        let mut resp = ProxyResponse::new(StatusCode::OK, HeaderMap::new());
        er.apply(&ctx(), &mut resp).await.unwrap();
        assert!(resp.replacement().is_none());
    }

    #[test]
    fn from_spec_rejects_invalid_status() {
        assert!(ErrorRemap::from_spec(&ErrorRemapSpec {
            when_status: 9999,
            error: "x".into(),
            detail: None
        })
        .is_err());
    }
}
