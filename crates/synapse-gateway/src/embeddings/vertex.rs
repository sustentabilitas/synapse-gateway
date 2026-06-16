//! Native Vertex AI embeddings via the publisher `:predict` endpoint.
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::embeddings::{EmbedOut, EmbeddingProvider};
use crate::error::GatewayError;
use crate::providers::vertex_auth::VertexAuth;

pub const VERTEX_EMBED_BATCH: usize = 250;

pub struct VertexEmbedder {
    auth: Arc<VertexAuth>,
    project: String,
    region: String,
    endpoint_base: String,
    client: reqwest::Client,
}

impl VertexEmbedder {
    pub fn new(auth: Arc<VertexAuth>, project: String, region: String, timeout: Duration) -> Self {
        let endpoint_base = if region == "global" {
            "https://aiplatform.googleapis.com".to_string()
        } else {
            format!("https://{region}-aiplatform.googleapis.com")
        };
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .expect("reqwest client");
        Self {
            auth,
            project,
            region,
            endpoint_base,
            client,
        }
    }

    fn predict_url(&self, model: &str) -> String {
        format!(
            "{}/v1/projects/{}/locations/{}/publishers/google/models/{}:predict",
            self.endpoint_base, self.project, self.region, model
        )
    }
}

#[derive(Serialize)]
struct PredictInstance {
    content: String,
}

#[derive(Serialize)]
struct PredictParams {
    #[serde(rename = "outputDimensionality")]
    output_dimensionality: u32,
}

#[derive(Serialize)]
pub struct PredictBody {
    instances: Vec<PredictInstance>,
    parameters: PredictParams,
}

pub fn build_predict_body(inputs: &[String], dims: u32) -> PredictBody {
    PredictBody {
        instances: inputs
            .iter()
            .map(|c| PredictInstance { content: c.clone() })
            .collect(),
        parameters: PredictParams {
            output_dimensionality: dims,
        },
    }
}

#[derive(Deserialize)]
struct RespStats {
    #[serde(default)]
    token_count: u64,
}

#[derive(Deserialize)]
struct RespEmbeddings {
    values: Vec<f32>,
    #[serde(default)]
    statistics: Option<RespStats>,
}

#[derive(Deserialize)]
struct RespPrediction {
    embeddings: RespEmbeddings,
}

#[derive(Deserialize)]
struct PredictResp {
    predictions: Vec<RespPrediction>,
}

pub fn parse_predict_response(raw: serde_json::Value) -> Result<EmbedOut, GatewayError> {
    let parsed: PredictResp = serde_json::from_value(raw).map_err(|e| GatewayError::Upstream {
        status: 502,
        body: format!("vertex embed parse: {e}"),
    })?;
    let input_tokens = parsed
        .predictions
        .iter()
        .map(|p| {
            p.embeddings
                .statistics
                .as_ref()
                .map(|s| s.token_count)
                .unwrap_or(0)
        })
        .sum();
    let vectors = parsed
        .predictions
        .into_iter()
        .map(|p| p.embeddings.values)
        .collect();
    Ok(EmbedOut {
        vectors,
        input_tokens,
    })
}

#[async_trait::async_trait]
impl EmbeddingProvider for VertexEmbedder {
    async fn embed(
        &self,
        model: &str,
        inputs: &[String],
        dims: u32,
    ) -> Result<EmbedOut, GatewayError> {
        // Mirror the chat lane (`src/vertex_native.rs`): fetch the cached bearer
        // via `VertexAuth::token()` (returns `Result<String, String>`).
        let token = self
            .auth
            .token()
            .await
            .map_err(|e| GatewayError::Upstream {
                status: 502,
                body: format!("vertex auth: {e}"),
            })?;
        let resp = self
            .client
            .post(self.predict_url(model))
            .bearer_auth(token)
            .json(&build_predict_body(inputs, dims))
            .send()
            .await
            .map_err(|e| GatewayError::Upstream {
                status: 502,
                body: format!("vertex embed send: {e}"),
            })?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(GatewayError::Upstream { status, body });
        }
        let raw = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| GatewayError::Upstream {
                status: 502,
                body: format!("vertex embed body: {e}"),
            })?;
        parse_predict_response(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_request_body_with_output_dimensionality() {
        let body = build_predict_body(&["hello".to_string(), "world".to_string()], 768);
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(v["instances"][0]["content"], "hello");
        assert_eq!(v["instances"][1]["content"], "world");
        assert_eq!(v["parameters"]["outputDimensionality"], 768);
    }
    #[test]
    fn parses_predictions_and_tokens() {
        let raw = serde_json::json!({
            "predictions": [
                { "embeddings": { "values": [0.1, 0.2], "statistics": { "token_count": 3 } } },
                { "embeddings": { "values": [0.3, 0.4], "statistics": { "token_count": 4 } } }
            ]
        });
        let out = parse_predict_response(raw).unwrap();
        assert_eq!(out.vectors.len(), 2);
        assert_eq!(out.vectors[0], vec![0.1, 0.2]);
        assert_eq!(out.input_tokens, 7);
    }
}
