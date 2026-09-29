---
sidebar_position: 5
title: Proxy metrics
description: What synapse-proxy exports on its metrics listener, and the queries to start with.
---

The proxy records OpenTelemetry metrics and serves them in Prometheus format at
`GET /metrics` on `metrics_addr` (default `0.0.0.0:9090`). The binary doesn't push OTLP; an
application that embeds the proxy library can add an OTLP exporter with
`Metrics::with_otlp`.

## What is recorded

| Prometheus name | Counts or measures |
|---|---|
| `synapse_proxy_requests_total_total` | Every request, by `route`, `method`, `status` and `outcome`. |
| `synapse_proxy_request_duration_seconds` | Time until the response headers were ready, by `route` and `method`. |
| `synapse_proxy_upstream_retries_total_total` | Retried upstream sends, by `route` and `reason`. |
| `synapse_proxy_upstream_errors_total_total` | Upstream sends that failed after every retry, by `route` and `reason`. |
| `synapse_proxy_transform_errors_total_total` | Requests stopped by a request or response step, by `route` and `transform` (`request` or `response`). |

`outcome` is `forwarded`, `no_route`, `context_unbound`, `transform_rejected` or
`upstream_error`, and `route` is `none` when no route matched. The exact labels and their
values are in the [metrics catalogue](../../reference/metrics-catalogue.md#synapse-proxy).

:::note
The counters really do end in `_total_total`: the proxy uses the OpenTelemetry Prometheus
exporter's default settings, which add a second suffix. The duration histogram's buckets are
sized for milliseconds while its values are seconds, so use `_sum / _count` for latency, not
quantiles. The [catalogue](../../reference/metrics-catalogue.md#synapse-proxy) explains both.
:::

## Queries to start with

Requests that were not forwarded, by reason:

```text
sum by (outcome) (rate(synapse_proxy_requests_total_total{outcome!="forwarded"}[5m]))
```

Requests refused because the context isn't bound, a sign that the control plane hasn't bound
an identity yet or that the binding expired:

```text
sum by (route) (rate(synapse_proxy_requests_total_total{outcome="context_unbound"}[5m]))
```

Server errors that each upstream returned, which the proxy passes through, next to the
upstream sends that failed outright:

```text
sum by (route, status) (rate(synapse_proxy_requests_total_total{outcome="forwarded", status=~"5.."}[5m]))
sum by (route) (rate(synapse_proxy_upstream_errors_total_total[5m]))
```

Average latency per route:

```text
sum by (route) (rate(synapse_proxy_request_duration_seconds_sum[5m]))
  / sum by (route) (rate(synapse_proxy_request_duration_seconds_count[5m]))
```

## Logs

The proxy logs with `tracing` to standard output, filtered by `RUST_LOG` (default `info`). It
logs its listen addresses at startup, a warning for each retried and each failed upstream
send, and a line when it starts draining.
