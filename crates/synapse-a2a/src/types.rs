use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Registration body from ploutonion (and future providers).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisterA2aAgentRequest {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Absolute URL where A2A JSON-RPC is served (ploutonion agent URL).
    pub endpoint_url: String,
    /// Absolute URL for the agent card JSON (may be gateway or origin).
    pub card_url: String,
    pub tags: Vec<String>,
    /// Full A2A agent-card JSON object (passthrough).
    pub card: Value,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct A2aCatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub card_url: String,
    pub endpoint_url: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct A2aCatalog {
    pub version: String,
    pub agents: Vec<A2aCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct A2aResolveResponse {
    pub id: String,
    pub endpoint_url: String,
    pub card_url: String,
    pub card: Value,
}
