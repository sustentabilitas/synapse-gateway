//! Native TypeSafe System One (Jev) lane. Jev evaluates typed questions
//! (choice / score / noul) against a state and returns structured decisions —
//! it has no chat surface, so the gateway exposes it as a verbatim passthrough
//! (`POST /typesafe/v1/systemone`, see `server::jev_passthrough`).

use std::collections::HashMap;
use std::time::Duration;

use serde_json::Value;

use crate::error::GatewayError;

/// TypeSafe's hosted API. Override with `TYPESAFE_BASE_URL` (self-hosted
/// deployments, tests).
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
/// Model id resolving to the current stable Jev build, used when the client
/// omits `model` from the request body.
pub const DEFAULT_MODEL: &str = "jev-latest";

#[derive(Debug, Clone)]
pub struct JevNativeProvider {
    http: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl JevNativeProvider {
    pub fn new(api_key: String, base_url: Option<String>, request_timeout: Duration) -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(request_timeout)
                .build()
                .unwrap(),
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
        }
    }

    /// The lane is available when `TYPESAFE_API_KEY` is set and non-empty;
    /// `TYPESAFE_BASE_URL` overrides the hosted endpoint.
    pub fn from_env(env: &HashMap<String, String>, request_timeout: Duration) -> Option<Self> {
        let api_key = env
            .get("TYPESAFE_API_KEY")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())?;
        let base_url = env
            .get("TYPESAFE_BASE_URL")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        Some(Self::new(api_key.to_string(), base_url, request_timeout))
    }

    /// Forward a System One request body verbatim. The caller renders the
    /// upstream status/body — successes and errors both pass through
    /// untranslated, like `VertexNativeProvider::passthrough_request`.
    pub async fn evaluate(&self, body: Value) -> Result<reqwest::Response, GatewayError> {
        self.http
            .post(format!("{}/v1/systemone", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| GatewayError::Upstream {
                status: 502,
                body: e.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_is_none_without_key() {
        assert!(JevNativeProvider::from_env(&HashMap::new(), Duration::from_secs(1)).is_none());
        let blank = HashMap::from([("TYPESAFE_API_KEY".to_string(), "  ".to_string())]);
        assert!(JevNativeProvider::from_env(&blank, Duration::from_secs(1)).is_none());
    }

    #[test]
    fn from_env_picks_up_key_and_base_url_override() {
        let env = HashMap::from([
            ("TYPESAFE_API_KEY".to_string(), "key".to_string()),
            (
                "TYPESAFE_BASE_URL".to_string(),
                "http://localhost:9".to_string(),
            ),
        ]);
        let provider = JevNativeProvider::from_env(&env, Duration::from_secs(1)).unwrap();
        assert_eq!(provider.base_url, "http://localhost:9");
    }

    #[test]
    fn defaults_to_the_hosted_api() {
        let provider = JevNativeProvider::new("key".into(), None, Duration::from_secs(1));
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
    }
}
