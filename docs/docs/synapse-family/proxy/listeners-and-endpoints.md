---
sidebar_position: 4
title: Listeners and endpoints
description: The three synapse-proxy listeners, how requests are matched and forwarded, the admin and metrics endpoints, and every error the proxy returns.
---

The proxy serves three listeners at once, each on its own address:

| Listener | Config key | Default | Serves |
|---|---|---|---|
| Data plane | `addr` | `0.0.0.0:8787` | Health probes and forwarded traffic. |
| Admin | `admin_addr` | `127.0.0.1:8788` | `POST` and `DELETE /internal/bind`. |
| Metrics | `metrics_addr` | `0.0.0.0:9090` | `GET /metrics`. |

The admin listener has no authentication: whoever can reach it can bind any identity. Keep
it on `127.0.0.1`, so only processes in the same container or pod can call it. The Docker
image declares only port `8787`; publish the others explicitly if you need them.

## Data plane

### Health probes

- `GET /healthz/liveness` always returns `200`.
- `GET /healthz/readiness` returns `200`, or `503` once the proxy is shutting down.

These two paths are answered by the proxy itself, never forwarded.

### Forwarding

Every other request is forwarded:

1. **Match.** The proxy picks the route with the longest `path_prefix` that the path starts
   with, among routes whose `methods` allow the request's method. Matching is by plain
   string prefix, so `/v1/orders` also matches `/v1/orders-archive`; end a prefix with `/`
   if that matters.
2. **Check context.** Every key in `require_context` must be bound.
3. **Read the body.** The request body is read in full, up to 64 MiB, so request bodies are
   not streamed.
4. **Transform.** Static `headers`, then `request_steps`, run in order.
5. **Forward.** The URL is `upstream` (without a trailing `/`) followed by the path, minus
   `path_prefix` when `strip_prefix` is set, and the original query string. The caller's
   headers are forwarded, except hop-by-hop headers (`Connection`, `Keep-Alive`,
   `Proxy-Connection`, `Transfer-Encoding`, `Upgrade`, `TE`, `Trailer`) and `Host`, which
   is set for the upstream.
6. **Respond.** `response_steps` run on the upstream's status and headers. Unless a step
   replaced the body, the upstream body is streamed back as it arrives, with the upstream's
   status and headers minus the hop-by-hop ones.

See [Configuration](./configuration.md#timeouts-and-retries) for timeouts and retries.

### Errors

Errors the proxy generates have a JSON body with `error` and `detail`:

| Status | `error` | When |
|---|---|---|
| `400` | `invalid_body` | A body step needs JSON and the body isn't valid JSON. |
| `404` | `no_route` | No route matches the path and method. |
| `413` | `body_too_large` | The request body is over 64 MiB. |
| `500` | `transform_error` | A custom transform failed with `TransformError::Internal`. |
| `502` | `request_failed` | The upstream couldn't be reached after every retry. |
| `503` | `request_failed` | A `require_context` key isn't bound (`"detail": "context not bound"`). |

A custom transform can also reject a request with a status and `error` of its own; see
[Custom transforms](./context-and-transforms.md#custom-transforms). Errors from the upstream
are passed through unchanged unless an `error_remap` step matches them.

## Admin

The admin listener sets the overlay layer of the context; see
[Context sources](./context-and-transforms.md#context-sources).

### `POST /internal/bind`

```json
{"values": {"org": "acme", "workspace": "team-a"}, "ttl_seconds": 3600}
```

`values` is an object of string keys and string values. `ttl_seconds` is optional; without it
the binding doesn't expire. The new overlay replaces the previous one. Returns `204`.

### `DELETE /internal/bind`

Removes the overlay, so the context reverts to the values built at startup. Returns `204`.

## Metrics

`GET /metrics` on the metrics listener returns the Prometheus text format. See
[Metrics](./metrics.md).

## Shutdown

On `SIGTERM` or `Ctrl-C`, the proxy starts answering `/healthz/readiness` with `503` so load
balancers stop sending it traffic. About a second later all three listeners stop accepting
connections, and the proxy exits once in-flight requests have finished.
