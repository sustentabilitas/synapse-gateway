---
slug: opentelemetry-metrics
title: "OpenTelemetry metrics in Synapse"
authors: [rajwilkhu]
tags: [observability]
description: Synapse 0.5.38 records its synapse_* metrics with opentelemetry-rust, serves them in Prometheus format, can push them over OTLP, and ships a Grafana dashboard to chart them.
---

As of release 0.5.38, the Synapse gateway records its metrics with
[opentelemetry-rust](https://github.com/open-telemetry/opentelemetry-rust) instead of the
`metrics` crate. Your Prometheus scrape keeps working with the same series names and labels,
and setting one environment variable now pushes the same metrics to an OpenTelemetry
collector.

This post covers why we moved, what the exposition looks like, what the metrics cover and how
to get the example Grafana dashboard running.

<!-- truncate -->

## Why opentelemetry-rust

The gateway was the odd one out in its own workspace. `synapse-proxy` and `synapse-mcp`
already recorded through OpenTelemetry instruments, while the gateway used the `metrics`
crate and its own exporter. Moving the gateway onto opentelemetry-rust 0.32 puts all three
crates on one pipeline: instruments created from an OpenTelemetry `Meter`, exported by the
OpenTelemetry Prometheus exporter, with OTLP available beside it.

That matters most when you embed the gateway. A Rust service that builds a `Gateway` in code
now passes it a `GatewayMetrics` made from a meter of its own `MeterProvider`, and the
gateway's instruments land wherever that provider exports. The default is a no-op, so an
embedded gateway records nothing until you opt in; a global `metrics` recorder in the host
process no longer receives gateway series. With the `server` feature,
`synapse::telemetry::install` builds exactly the exporters the binary uses. See
[Embedding Synapse as a library](/docs/guides/embedding-as-library/#what-the-builder-leaves-out).

## Prometheus by default, OTLP when you ask

The binary always serves Prometheus text on its metrics listener, `SYNAPSE_METRICS_ADDR`,
which defaults to `0.0.0.0:9090`, at `GET /metrics`. The API port does not serve metrics.

```bash
curl -s http://localhost:9090/metrics | grep '^synapse_'
```

Set `OTEL_EXPORTER_OTLP_ENDPOINT` to a collector's base URL, such as
`http://otel-collector:4318`, and the gateway also pushes the same metrics over OTLP/HTTP to
`<endpoint>/v1/metrics`, every 60 seconds by default, tagged with `service.name` from
`OTEL_SERVICE_NAME` (default `synapse-gateway`). Both paths carry the same names and labels.
The startup line `synapse-gateway metrics listening` tells you whether OTLP is on. The
[Telemetry](/docs/configuration/environment-variables/#telemetry) variables are listed with
the rest of the configuration.

## An exposition that looks hand-written

The OpenTelemetry Prometheus exporter adds things by default that would have broken existing
dashboards: a second `_total` on counters, unit suffixes, `otel_scope_*` labels and a
`target_info` series. The gateway turns all of those off, so names come out exactly as the
catalogue lists them, such as `synapse_requests_total` and `synapse_request_duration_seconds`.

Two things did change, both deliberately:

- **Durations are histograms.** The `*_duration_seconds` metrics now expose `_bucket`, `_sum`
  and `_count` series, with buckets from 5 ms to 120 seconds, instead of summaries. Queries on
  `{quantile=...}` move to `histogram_quantile(...)` over `_bucket`, and you can now aggregate
  latency across instances, which summaries never allowed.
- **Cardinality is capped.** Each metric keeps at most 2,000 label combinations, the
  OpenTelemetry SDK default. Beyond that, new combinations fold into one series labelled
  `otel_metric_overflow="true"`.

## What the metrics cover

The [metrics catalogue](/docs/reference/metrics-catalogue/) lists every instrument with its
type and labels. In short, the gateway counts:

- **Chat requests:** requests, latency, and input and output tokens, labelled by `route`,
  serving `model`, `lane` and `system`, the provider family in OpenLLMetry's `gen_ai.system`
  vocabulary (`vertexai`, `openai`, `dashscope`, `oai_compat`).
- **Embeddings and passthrough:** their own request and latency series, including the Gemini
  and Jev passthrough endpoints.
- **Jev:** routing decisions by tier and outcome, decision latency, and hybrid extraction
  responses.
- **Guardrails:** scans by outcome, matches by scanner and severity, and scan latency.
- **The cost ledger:** rows dropped because the queue was full, and rows a sink failed to
  write.

Two things are deliberately missing. Tenants are not labels, because their values come from
clients and are unbounded; per-tenant usage and cost live in the
[cost ledger](/docs/guides/cost-ledger/), which you can query directly. And there is no
metric for failed chat completions: the request metrics count requests that produced a
response, so measure error rates at the load balancer or mesh in front of the gateway.
[What the request metrics count](/docs/operating/metrics/#what-the-request-metrics-count)
spells out the edge cases, such as abandoned streams.

The gateway records metrics only. It creates no OpenTelemetry trace spans; logs go through
`tracing`, filtered with `RUST_LOG`. See [Logging](/docs/operating/logging/#tracing).

## A dashboard to start from

The repository ships an example Grafana dashboard,
[synapse-gateway-dashboard.json](pathname:///dashboards/synapse-gateway-dashboard.json). It
charts traffic by route, lane and model; latency percentiles; token rates; cost-ledger health;
and embeddings. The latency panels are the ones the move to histograms fixed: they query
`histogram_quantile` over `_bucket` series, which the old summaries never produced, so they
used to stay empty.

Two things to check before you import it: every panel names its Prometheus data source by UID
`prometheus`, and every query filters on a `$job` variable that defaults to `synapse-gateway`.
The [Grafana dashboard](/docs/operating/grafana-dashboard/) page covers importing it by hand,
provisioning it with Docker Compose, and loading it on Kubernetes through the
kube-prometheus-stack sidecar. Its Resilience row stays empty, because chat legs have no
retries or circuit breakers to report on.

## Where to start

Point Prometheus at port 9090, import the dashboard, and add the starter alerts from
[Metrics](/docs/operating/metrics/#alerts-to-start-with): lost ledger rows, a failing ledger
sink and degraded Jev routing are problems no client error will tell you about.
