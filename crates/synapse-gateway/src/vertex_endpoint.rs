//! Vertex AI REST host resolution for global, multi-region, and single-region locations.

/// Resolve the Vertex AI API base URL for a location id.
///
/// - `global` → `https://aiplatform.googleapis.com`
/// - Multi-region `us` / `eu` → `https://aiplatform.{us|eu}.rep.googleapis.com`
/// - Single region (e.g. `us-central1`) → `https://{region}-aiplatform.googleapis.com`
///
/// See <https://docs.cloud.google.com/gemini-enterprise-agent-platform/resources/locations>.
pub fn vertex_endpoint_base(region: &str) -> String {
    match region {
        "global" => "https://aiplatform.googleapis.com".into(),
        "us" | "eu" => format!("https://aiplatform.{region}.rep.googleapis.com"),
        _ => format!("https://{region}-aiplatform.googleapis.com"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_host() {
        assert_eq!(
            vertex_endpoint_base("global"),
            "https://aiplatform.googleapis.com"
        );
    }

    #[test]
    fn multi_region_us_and_eu_hosts() {
        assert_eq!(
            vertex_endpoint_base("us"),
            "https://aiplatform.us.rep.googleapis.com"
        );
        assert_eq!(
            vertex_endpoint_base("eu"),
            "https://aiplatform.eu.rep.googleapis.com"
        );
    }

    #[test]
    fn single_region_host() {
        assert_eq!(
            vertex_endpoint_base("us-central1"),
            "https://us-central1-aiplatform.googleapis.com"
        );
        assert_eq!(
            vertex_endpoint_base("europe-west1"),
            "https://europe-west1-aiplatform.googleapis.com"
        );
    }
}
