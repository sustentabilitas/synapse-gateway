---
sidebar_position: 1
title: Metrics
description: How the Synapse gateway exports its synapse_* metrics to Prometheus and OpenTelemetry collectors, what they count, and which queries and alerts to start with.
---

The gateway records metrics about every request it serves: traffic and latency per route,
model and lane, token usage, routing decisions, guardrail scans and the health of the cost
ledger. Scrape them with Prometheus, or push them to an OpenTelemetry collector, to build
dashboards and alerts. Per-tenant usage and cost are not metrics; query the
[cost ledger](../guides/cost-ledger.md) for those.

## How metrics are exported

The gateway records its metrics with OpenTelemetry and exports them two ways:

- **Prometheus**, always. `GET /metrics` on the metrics listener, `SYNAPSE_METRICS_ADDR`
  (default `0.0.0.0:9090`), returns the Prometheus text format. `GET /` on the same port
  returns the same. The API port does not serve metrics.
- **OTLP/HTTP**, when `OTEL_EXPORTER_OTLP_ENDPOINT` is set to a collector's base URL, such as
  `http://otel-collector:4318`. The gateway pushes to `<endpoint>/v1/metrics` every 60
  seconds, or every `OTEL_METRIC_EXPORT_INTERVAL` milliseconds, with the resource attribute
  `service.name` set from `OTEL_SERVICE_NAME` (default `synapse-gateway`).

Both paths carry the same metrics with the same names and labels. The startup log line
`synapse-gateway metrics listening` shows the metrics address and whether OTLP is on
(`otlp=true`). See [Telemetry](../configuration/environment-variables.md#telemetry) for the
variables.

Check what the gateway exports:

```bash
curl -s http://localhost:9090/metrics | grep '^synapse_'
```

A metric appears only after it has been recorded once, so a freshly started gateway shows
few series.

## Exposition format

The Prometheus output is kept close to what hand-written Prometheus instrumentation would
produce:

- Metric names are exactly as listed, with no extra `_total` or unit suffixes, and no
  `otel_scope_*` labels or `target_info` series.
- Durations are histograms in seconds (`_bucket`, `_sum` and `_count` series). The
  [metrics catalogue](../reference/metrics-catalogue.md#gateway) lists the bucket boundaries.
- Each metric keeps at most 2,000 label combinations. Beyond that, new combinations are
  folded into a single series labelled `otel_metric_overflow="true"`.

## What the metrics cover

The [metrics catalogue](../reference/metrics-catalogue.md) lists every metric with its type,
unit, labels and label values.

| Area | Metrics |
|---|---|
| Chat requests | `synapse_requests_total`, `synapse_request_duration_seconds`, `synapse_input_tokens_total`, `synapse_output_tokens_total`, labelled by `route`, `model`, `system` and `lane`. |
| Embeddings | `synapse_embeddings_total`, `synapse_embedding_duration_seconds`. See [Embeddings](../guides/embeddings.md#metrics). |
| Passthrough | `synapse_passthrough_total`, `synapse_passthrough_fallback_total`, for the Gemini and Jev passthrough endpoints. |
| Jev | `synapse_routing_decisions_total`, `synapse_routing_decision_duration_seconds` for the [Jev router](../guides/jev-router.md#metrics-and-logs); `synapse_jev_extraction_total` for [hybrid extraction](../guides/jev-lane.md#ledger-and-metrics). |
| Guardrails | `synapse_guard_scans_total`, `synapse_guard_matches_total`, `synapse_guard_scan_duration_seconds`. See [Guardrails policy](../configuration/guardrails-policy.md#metrics). |
| Cost ledger | `synapse_ledger_dropped_total`, `synapse_ledger_errors_total`. See [Delivery](../guides/cost-ledger.md#delivery). |

Tenant, workspace, user and thread are deliberately not labels: their values come from
clients and are unbounded, and every new value would create new series. Use the ledger for
per-tenant numbers.

### What the request metrics count

`synapse_requests_total` and its companions count chat completions that produced a response:

- A non-streaming request is counted once it succeeds, with the latency of the whole request
  and the model of the leg that served it.
- A streaming request is counted when its stream ends, including streams that fail after the
  first chunk and streams the client abandons. Its latency runs until the stream ends.
- A request that fails before any response, such as an unknown model, a guardrail block or
  a chain where every leg failed, is not counted. Neither are Jev router decisions, embeddings
  or passthrough calls, which have their own metrics.
- A hybrid extraction request is counted once for its Jev answer and once for each
  extraction.

There is no metric for failed chat completions. Measure error rates at the load balancer,
ingress or service mesh in front of the gateway.

## Queries

Request rate per route and lane:

```text
sum by (route, lane) (rate(synapse_requests_total[5m]))
```

95th percentile latency per route:

```text
histogram_quantile(0.95, sum by (le, route) (rate(synapse_request_duration_seconds_bucket[5m])))
```

Output tokens per second per serving model:

```text
sum by (model) (rate(synapse_output_tokens_total[5m]))
```

Which models serve each route. The `model` label names the leg that served each request, so
traffic on a model other than the route's first leg means the chain is falling back:

```text
sum by (route, model) (rate(synapse_requests_total[1h]))
```

Share of Jev router decisions that fell back to `default_tier`:

```text
sum by (route) (rate(synapse_routing_decisions_total{outcome=~"timeout|error|low_confidence"}[15m]))
  / sum by (route) (rate(synapse_routing_decisions_total[15m]))
```

## Alerts to start with

| Alert on | Expression | Why |
|---|---|---|
| Lost ledger rows | `increase(synapse_ledger_dropped_total[10m]) > 0` | The ledger queue overflowed and usage was not recorded. |
| Failing ledger sink | `sum by (backend) (increase(synapse_ledger_errors_total[10m])) > 0` | A sink is rejecting writes; rows sent to it are lost. |
| Degraded Jev routing | The decision ratio above `> 0.2` | Requests are landing on `default_tier` without a real decision. |
| Passthrough errors | `sum by (provider) (rate(synapse_passthrough_total{status="error"}[5m])) > 0` | Gemini or Jev SDK clients are getting errors. |
| Guardrail blocks | `sum by (policy) (rate(synapse_guard_scans_total{outcome="block"}[15m]))` above your baseline | A spike can mean an attack, or a policy blocking legitimate traffic. |
| Gateway down | `up{job="synapse-gateway"} == 0` | Prometheus cannot scrape the gateway. |

## Embedded gateways

A gateway built with `Gateway::builder()` records no metrics unless you pass it a
`GatewayMetrics`. See
[What the builder leaves out](../guides/embedding-as-library.md#what-the-builder-leaves-out).
