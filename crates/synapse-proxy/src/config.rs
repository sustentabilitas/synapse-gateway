//! Proxy config: listen address + passthrough routes, from TOML.

use serde::Deserialize;
use std::collections::HashMap;

/// One passthrough route: requests whose path starts with `path_prefix` are
/// forwarded to `upstream`, optionally stripping the prefix, with `headers`
/// injected on the forwarded request.
#[derive(Debug, Clone, Deserialize)]
pub struct Route {
    pub path_prefix: String,
    pub upstream: String,
    #[serde(default)]
    pub strip_prefix: bool,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

/// Proxy configuration. `addr` defaults to `0.0.0.0:8787`.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_addr")]
    pub addr: String,
    #[serde(default)]
    pub routes: Vec<Route>,
}

fn default_addr() -> String {
    "0.0.0.0:8787".to_string()
}

impl Config {
    /// Load from the TOML file at `SYNAPSE_PROXY_CONFIG_PATH` (default
    /// `synapse-proxy.toml`); `SYNAPSE_PROXY_ADDR` overrides the listen address.
    pub fn load() -> anyhow::Result<Self> {
        let path = std::env::var("SYNAPSE_PROXY_CONFIG_PATH")
            .unwrap_or_else(|_| "synapse-proxy.toml".to_string());
        let content =
            std::fs::read_to_string(&path).map_err(|e| anyhow::anyhow!("reading {path}: {e}"))?;
        let mut config = Self::from_toml_str(&content)?;
        if let Ok(addr) = std::env::var("SYNAPSE_PROXY_ADDR") {
            if !addr.trim().is_empty() {
                config.addr = addr;
            }
        }
        Ok(config)
    }

    pub fn from_toml_str(s: &str) -> anyhow::Result<Self> {
        toml::from_str(s).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        addr = "127.0.0.1:9000"
        [[routes]]
        path_prefix = "/v1/llm"
        upstream = "http://gw:8080"
        strip_prefix = true
        [routes.headers]
        x-forwarded-by = "synapse-proxy"
        [[routes]]
        path_prefix = "/v1/tools"
        upstream = "http://tools:7000"
    "#;

    #[test]
    fn parses_addr_routes_and_headers() {
        let c = Config::from_toml_str(SAMPLE).unwrap();
        assert_eq!(c.addr, "127.0.0.1:9000");
        assert_eq!(c.routes.len(), 2);
        assert_eq!(c.routes[0].path_prefix, "/v1/llm");
        assert_eq!(c.routes[0].upstream, "http://gw:8080");
        assert!(c.routes[0].strip_prefix);
        assert_eq!(
            c.routes[0]
                .headers
                .get("x-forwarded-by")
                .map(String::as_str),
            Some("synapse-proxy")
        );
    }

    #[test]
    fn defaults_addr_and_strip_prefix_and_headers() {
        let c = Config::from_toml_str(
            r#"
            [[routes]]
            path_prefix = "/a"
            upstream = "http://u"
        "#,
        )
        .unwrap();
        assert_eq!(c.addr, "0.0.0.0:8787"); // default
        assert!(!c.routes[0].strip_prefix); // default false
        assert!(c.routes[0].headers.is_empty()); // default empty
    }
}
