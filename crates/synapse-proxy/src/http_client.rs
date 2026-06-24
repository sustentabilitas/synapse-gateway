use std::time::Duration;

/// Build the shared reqwest client used for upstream forwarding.
///
/// Without explicit timeouts a broken MCS/DNS path can hang until the caller
/// gives up (e.g. sandbox `python` exit 124) while the upstream never logs a
/// request. Configure via:
///
/// - `SYNAPSE_PROXY_UPSTREAM_CONNECT_TIMEOUT_SECS` (default 10)
/// - `SYNAPSE_PROXY_UPSTREAM_TIMEOUT_SECS` (default 120)
pub fn build_http_client() -> reqwest::Result<reqwest::Client> {
    let connect_secs = std::env::var("SYNAPSE_PROXY_UPSTREAM_CONNECT_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);
    let request_secs = std::env::var("SYNAPSE_PROXY_UPSTREAM_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);

    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(connect_secs))
        .timeout(Duration::from_secs(request_secs))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_client_with_defaults() {
        build_http_client().expect("client");
    }
}
