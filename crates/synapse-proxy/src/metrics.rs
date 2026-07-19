//! OpenTelemetry metrics exported in Prometheus text format on /metrics.

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use opentelemetry::metrics::{Counter, Histogram, Meter, MeterProvider as _};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use opentelemetry_sdk::Resource;
use prometheus::{Encoder, Registry, TextEncoder};

pub struct Metrics {
    requests: Counter<u64>,
    duration: Histogram<f64>,
    upstream_errors: Counter<u64>,
    transform_errors: Counter<u64>,
    provider: SdkMeterProvider,
}

impl Metrics {
    /// Prometheus-only (unchanged behaviour). Retained for existing callers.
    pub fn new() -> anyhow::Result<(Self, Registry)> {
        Self::with_otlp(None, "synapse-proxy")
    }

    /// Prometheus reader (always, exported on `:9090`) plus an OTLP/HTTP
    /// `PeriodicReader` when `otlp_endpoint` is `Some`. `service_name` is
    /// attached as the `service.name` resource attribute so OTLP series carry
    /// e.g. `service_name=sandbox-broker`.
    ///
    /// `otlp_endpoint` is a base collector URL like `http://host:4318`; the
    /// `/v1/metrics` signal path is appended here (a programmatically-supplied
    /// endpoint is used verbatim by opentelemetry-otlp 0.32, so we must append
    /// the path ourselves).
    pub fn with_otlp(
        otlp_endpoint: Option<&str>,
        service_name: &str,
    ) -> anyhow::Result<(Self, Registry)> {
        let registry = Registry::new();
        let prom = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .build()?;
        let resource = Resource::builder()
            .with_service_name(service_name.to_string())
            .build();
        let mut builder = SdkMeterProvider::builder()
            .with_reader(prom)
            .with_resource(resource);
        if let Some(endpoint) = otlp_endpoint {
            let exporter = opentelemetry_otlp::MetricExporter::builder()
                .with_http()
                .with_endpoint(format!("{}/v1/metrics", endpoint.trim_end_matches('/')))
                .build()?;
            builder = builder.with_reader(PeriodicReader::builder(exporter).build());
        }
        let provider = builder.build();
        let meter = provider.meter("synapse-proxy");
        let metrics = Self {
            requests: meter.u64_counter("synapse_proxy_requests_total").build(),
            duration: meter
                .f64_histogram("synapse_proxy_request_duration_seconds")
                .build(),
            upstream_errors: meter
                .u64_counter("synapse_proxy_upstream_errors_total")
                .build(),
            transform_errors: meter
                .u64_counter("synapse_proxy_transform_errors_total")
                .build(),
            provider,
        };
        Ok((metrics, registry))
    }

    /// A meter on the SAME provider (so downstream crates such as synapse-mcp
    /// build instruments that share this provider's OTLP + Prometheus readers
    /// and single `service.name` resource).
    pub fn meter(&self) -> Meter {
        self.provider.meter("sandbox-broker")
    }

    pub fn record(&self, route: &str, method: &str, status: u16, outcome: &str, secs: f64) {
        let labels = [
            KeyValue::new("route", route.to_string()),
            KeyValue::new("method", method.to_string()),
            KeyValue::new("status", status.to_string()),
            KeyValue::new("outcome", outcome.to_string()),
        ];
        self.requests.add(1, &labels);
        self.duration.record(
            secs,
            &[
                KeyValue::new("route", route.to_string()),
                KeyValue::new("method", method.to_string()),
            ],
        );
    }

    pub fn upstream_error(&self, route: &str, reason: &str) {
        self.upstream_errors.add(
            1,
            &[
                KeyValue::new("route", route.to_string()),
                KeyValue::new("reason", reason.to_string()),
            ],
        );
    }

    pub fn transform_error(&self, route: &str, transform: &str) {
        self.transform_errors.add(
            1,
            &[
                KeyValue::new("route", route.to_string()),
                KeyValue::new("transform", transform.to_string()),
            ],
        );
    }
}

pub fn metrics_router(registry: Registry) -> Router {
    Router::new()
        .route("/metrics", get(serve))
        .with_state(registry)
}

async fn serve(State(registry): State<Registry>) -> impl IntoResponse {
    let mut buf = Vec::new();
    if TextEncoder::new()
        .encode(&registry.gather(), &mut buf)
        .is_err()
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, "encode error").into_response();
    }
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], buf).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_exports() {
        let (m, registry) = Metrics::new().unwrap();
        m.record("cortex", "POST", 200, "forwarded", 0.01);
        let mut buf = Vec::new();
        TextEncoder::new()
            .encode(&registry.gather(), &mut buf)
            .unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("synapse_proxy_requests_total"));
        assert!(text.contains("route=\"cortex\""));
    }

    #[test]
    fn otlp_and_prometheus_readers_coexist() {
        // With an OTLP endpoint set, the Prometheus registry still exports the proxy series.
        let (m, registry) =
            Metrics::with_otlp(Some("http://127.0.0.1:4318"), "sandbox-broker").unwrap();
        m.record("cortex", "POST", 200, "forwarded", 0.01);
        let mut buf = Vec::new();
        TextEncoder::new()
            .encode(&registry.gather(), &mut buf)
            .unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("synapse_proxy_requests_total"));
        // The exposed meter creates instruments on the same provider.
        let _c = m.meter().u64_counter("probe_total").build();
    }
}
