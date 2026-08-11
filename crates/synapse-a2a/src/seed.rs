//! Boot-time seed of `A2aRegistry` from TOML + HTTP card fetch.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use backon::{ExponentialBuilder, Retryable};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::registry::A2aRegistry;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct A2aSeedFile {
    #[serde(default)]
    pub a2a_agents: Vec<A2aSeedAgent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct A2aSeedAgent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub endpoint_url: String,
    pub card_url: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

pub fn parse_seed_toml(raw: &str) -> Result<A2aSeedFile> {
    toml::from_str(raw).context("parsing a2a seed TOML")
}

fn retryable_status(status: StatusCode) -> bool {
    status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
}

#[derive(Debug)]
enum CardFetchError {
    Retryable(anyhow::Error),
    Fatal(anyhow::Error),
}

impl std::fmt::Display for CardFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retryable(e) | Self::Fatal(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CardFetchError {}

async fn fetch_card(client: &reqwest::Client, card_url: &str) -> Result<Value> {
    let op = || async {
        let resp = client
            .get(card_url)
            .send()
            .await
            .map_err(|e| CardFetchError::Retryable(e.into()))?;
        let status = resp.status();
        if retryable_status(status) {
            return Err(CardFetchError::Retryable(anyhow::anyhow!(
                "HTTP {status} from {card_url}"
            )));
        }
        if !status.is_success() {
            return Err(CardFetchError::Fatal(anyhow::anyhow!(
                "HTTP {status} from {card_url}"
            )));
        }
        let card: Value = resp
            .json()
            .await
            .map_err(|e| CardFetchError::Fatal(e.into()))?;
        Ok(card)
    };

    // 3 attempts total: 1 initial + 2 retries (mirrors synapse-gateway's
    // `resilience.rs`, where `with_max_times` counts retries-after-first).
    let builder = ExponentialBuilder::default()
        .with_min_delay(Duration::from_millis(200))
        .with_max_delay(Duration::from_secs(5))
        .with_factor(2.0)
        .with_max_times(2)
        .with_jitter();

    op.retry(builder)
        .when(|e| matches!(e, CardFetchError::Retryable(_)))
        .notify(|e, dur| {
            warn!(error = %e, delay_ms = dur.as_millis(), card_url, "retrying a2a card fetch");
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Fetch cards and insert-only register each agent.
pub async fn seed_agents(
    registry: &A2aRegistry,
    agents: &[A2aSeedAgent],
    client: &reqwest::Client,
) -> Result<()> {
    for agent in agents {
        let card = fetch_card(client, &agent.card_url).await.with_context(|| {
            format!(
                "fetching agent card for id='{}' url='{}'",
                agent.id, agent.card_url
            )
        })?;
        let inserted = registry.try_register(
            agent.id.clone(),
            agent.name.clone(),
            agent.description.clone(),
            agent.endpoint_url.clone(),
            agent.card_url.clone(),
            agent.tags.clone(),
            card,
            agent.ttl_seconds.map(Duration::from_secs),
        );
        if inserted {
            info!(id = %agent.id, "seeded a2a agent");
        } else {
            warn!(id = %agent.id, "skipping duplicate a2a seed id");
        }
    }
    Ok(())
}

/// Load TOML from `path` and seed. Caller must only invoke when the path exists.
pub async fn seed_from_path(registry: &A2aRegistry, path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading a2a seed file {}", path.display()))?;
    let file = parse_seed_toml(&raw)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("building reqwest client for a2a seed")?;
    seed_agents(registry, &file.a2a_agents, &client).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_card() -> Value {
        serde_json::json!({
            "name": "GHG Emissions",
            "description": "d",
            "url": "http://ploutonion/a2a/agents/ghg-emissions",
            "version": "1.0",
            "skills": []
        })
    }

    #[test]
    fn parse_seed_toml_reads_agents() {
        let raw = r#"
[[a2a_agents]]
id = "ghg-emissions"
name = "GHG Emissions"
description = "Estimates GHG emissions"
endpoint_url = "http://ploutonion/a2a/agents/ghg-emissions"
card_url = "http://ploutonion/card.json"
tags = ["ghg"]
ttl_seconds = 3600
"#;
        let file = parse_seed_toml(raw).unwrap();
        assert_eq!(file.a2a_agents.len(), 1);
        assert_eq!(file.a2a_agents[0].id, "ghg-emissions");
        assert_eq!(file.a2a_agents[0].ttl_seconds, Some(3600));
    }

    #[test]
    fn parse_empty_or_comment_only_yields_zero_agents() {
        assert!(parse_seed_toml("").unwrap().a2a_agents.is_empty());
        assert!(parse_seed_toml("# only comments\n")
            .unwrap()
            .a2a_agents
            .is_empty());
    }

    #[test]
    fn parse_invalid_toml_errors() {
        assert!(parse_seed_toml("[[a2a_agents]\n").is_err());
    }

    #[tokio::test]
    async fn seed_fetches_card_and_registers() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/card.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(sample_card()))
            .mount(&server)
            .await;

        let registry = A2aRegistry::new();
        let agent = A2aSeedAgent {
            id: "ghg-emissions".into(),
            name: "GHG Emissions".into(),
            description: "d".into(),
            endpoint_url: "http://ploutonion/a2a/agents/ghg-emissions".into(),
            card_url: format!("{}/card.json", server.uri()),
            tags: vec!["ghg".into()],
            ttl_seconds: None,
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        seed_agents(&registry, &[agent], &client).await.unwrap();
        let got = registry.resolve("ghg-emissions").unwrap();
        assert_eq!(got.card["name"], "GHG Emissions");
        assert_eq!(
            got.endpoint_url,
            "http://ploutonion/a2a/agents/ghg-emissions"
        );
    }

    #[tokio::test]
    async fn seed_retries_5xx_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/card.json"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/card.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(sample_card()))
            .mount(&server)
            .await;

        let registry = A2aRegistry::new();
        let agent = A2aSeedAgent {
            id: "ghg-emissions".into(),
            name: "GHG".into(),
            description: "d".into(),
            endpoint_url: "http://p/a2a".into(),
            card_url: format!("{}/card.json", server.uri()),
            tags: vec![],
            ttl_seconds: None,
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        seed_agents(&registry, &[agent], &client).await.unwrap();
        assert!(registry.resolve("ghg-emissions").is_some());
    }

    #[tokio::test]
    async fn seed_fails_after_retry_exhaustion() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/card.json"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let registry = A2aRegistry::new();
        let agent = A2aSeedAgent {
            id: "ghg-emissions".into(),
            name: "GHG".into(),
            description: "d".into(),
            endpoint_url: "http://p/a2a".into(),
            card_url: format!("{}/card.json", server.uri()),
            tags: vec![],
            ttl_seconds: None,
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        let err = seed_agents(&registry, &[agent], &client).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("ghg-emissions"), "{msg}");
        assert!(
            msg.contains("card.json") || msg.contains("/card.json"),
            "{msg}"
        );
    }

    #[tokio::test]
    async fn seed_duplicate_id_skips_second() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/card.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(sample_card()))
            .expect(2)
            .mount(&server)
            .await;

        let registry = A2aRegistry::new();
        let make = || A2aSeedAgent {
            id: "ghg-emissions".into(),
            name: "GHG".into(),
            description: "d".into(),
            endpoint_url: "http://first/a2a".into(),
            card_url: format!("{}/card.json", server.uri()),
            tags: vec![],
            ttl_seconds: None,
        };
        let mut second = make();
        second.endpoint_url = "http://second/a2a".into();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        seed_agents(&registry, &[make(), second], &client)
            .await
            .unwrap();
        assert_eq!(
            registry.resolve("ghg-emissions").unwrap().endpoint_url,
            "http://first/a2a"
        );
    }

    #[tokio::test]
    async fn seed_non_retryable_4xx_fails_without_success() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/card.json"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;

        let registry = A2aRegistry::new();
        let agent = A2aSeedAgent {
            id: "missing".into(),
            name: "x".into(),
            description: "d".into(),
            endpoint_url: "http://p/a2a".into(),
            card_url: format!("{}/card.json", server.uri()),
            tags: vec![],
            ttl_seconds: None,
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap();
        assert!(seed_agents(&registry, &[agent], &client).await.is_err());
        assert!(registry.resolve("missing").is_none());
    }
}
