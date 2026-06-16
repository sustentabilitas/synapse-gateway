//! OpenTelemetry metrics exported in Prometheus text format on /metrics.

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use opentelemetry::metrics::{Counter, Histogram, MeterProvider as _};
use opentelemetry::KeyValue;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use prometheus::{Encoder, Registry, TextEncoder};

pub struct Metrics {
    requests: Counter<u64>,
    duration: Histogram<f64>,
    upstream_errors: Counter<u64>,
    transform_errors: Counter<u64>,
    _provider: SdkMeterProvider,
}

impl Metrics {
    /// Build the meter provider wired to a fresh Prometheus registry; returns both.
    pub fn new() -> anyhow::Result<(Self, Registry)> {
        let registry = Registry::new();
        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .build()?;
        let provider = SdkMeterProvider::builder().with_reader(exporter).build();
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
            _provider: provider,
        };
        Ok((metrics, registry))
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
}
