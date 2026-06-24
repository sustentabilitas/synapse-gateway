# synapse-proxy

[![crates.io](https://img.shields.io/crates/v/synapse-proxy.svg)](https://crates.io/crates/synapse-proxy)
[![Docker Hub](https://img.shields.io/docker/v/sustentabilitas/synapse-proxy?logo=docker&label=docker)](https://hub.docker.com/r/sustentabilitas/synapse-proxy)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](../../LICENSE)
[![CI](https://github.com/sustentabilitas/synapse-gateway/actions/workflows/ci.yml/badge.svg)](https://github.com/sustentabilitas/synapse-gateway/actions/workflows/ci.yml)

A config-driven reverse-proxy sidecar. It forwards incoming requests to upstreams
by longest matching `path_prefix`, optionally stripping the prefix, injecting
static headers, and streaming the response back.

## Configuration

Set `SYNAPSE_PROXY_CONFIG_PATH` (default `synapse-proxy.toml`); `SYNAPSE_PROXY_ADDR`
overrides the listen address.

Upstream HTTP client timeouts (avoid indefinite hangs on broken DNS/MCS paths):

| Variable | Default |
|---|---|
| `SYNAPSE_PROXY_UPSTREAM_CONNECT_TIMEOUT_SECS` | `10` |
| `SYNAPSE_PROXY_UPSTREAM_TIMEOUT_SECS` | `120` |

```toml
addr = "0.0.0.0:8787"
admin_addr = "127.0.0.1:8788"
metrics_addr = "0.0.0.0:9090"

[context]
env = { org = "BROKER_ORG_ID", workspace = "BROKER_WORKSPACE_ID" }

[[routes]]
name = "cortex"
path_prefix = "/v1/cortex"
upstream = "http://cortex:8080"
strip_prefix = true
require_context = ["org", "workspace"]
request_steps = [
  { inject = { header = "X-Tenant-Id",    from_context = "org" } },
  { inject = { header = "X-Workspace-Id", from_context = "workspace" } },
  { inject = { header = "X-User-Id",      const = "_default" } },
]

[[routes]]
name = "integration-call"
path_prefix = "/v1/integrations/call"
upstream = "http://integrations:8080/call-workspace"
strip_prefix = true
require_context = ["org", "workspace"]
request_steps = [
  { wrap = { under = "request", inject = [
      { body = "org",       from_context = "org" },
      { body = "workspace", from_context = "workspace" },
  ] } },
]
response_steps = [
  { error_remap = { when_status = 401, error = "auth_expired" } },
  { error_remap = { when_status = 404, error = "no_connection" } },
]
```

## Context & Transforms

### Context sources

The `[context]` table controls how the per-request context is populated.

- `static`: TOML literal key/value pairs baked in at startup.
- `env`: maps a context key to the name of the environment variable read at startup; `env` values win over `static` when both are present.
- At runtime, the admin endpoint (`/internal/bind`) can push a per-overlay that wins over both. `DELETE /internal/bind` reverts to the static/env baseline.

### Route fields

| field | description |
|---|---|
| `name` | Optional label used in metrics; defaults to `path_prefix`. |
| `path_prefix` | Longest-prefix match. |
| `upstream` | Forwarding target (scheme + host + optional path prefix). |
| `strip_prefix` | Remove `path_prefix` from the path before forwarding. |
| `methods` | Restrict to HTTP methods (e.g. `["POST"]`); empty = any. |
| `require_context` | List of context keys that must be present; returns `503 {"error":"request_failed","detail":"context not bound"}` if any are absent. |
| `request_steps` | Ordered pipeline of transforms applied to the outgoing request. |
| `response_steps` | Ordered pipeline of transforms applied to the upstream response. |

### Built-in transforms

**`inject`** — inject a value into a header or dotted body path.
```toml
{ inject = { header = "X-Tenant-Id", from_context = "org" } }
{ inject = { header = "X-User-Id",   const = "_default" } }
{ inject = { body = "tenant",        from_context = "org" } }
```
> **Security note:** When a `from_context` key is absent, header targets are fail-safe: the injector removes any caller-supplied value for that header so it cannot pass through. Body targets rely on `require_context` as the gate. Any `from_context` key used for identity (e.g. tenant or user headers) SHOULD also be listed in the route's `require_context` as defense in depth.

**`wrap`** — nest the incoming JSON body under a key and inject sibling fields.
```toml
{ wrap = { under = "request", inject = [
    { body = "org",       from_context = "org" },
    { body = "workspace", from_context = "workspace" },
] } }
```

**`error_remap`** — replace the upstream response body with a normalized error object when a status matches (status code is kept).
```toml
{ error_remap = { when_status = 401, error = "auth_expired" } }
{ error_remap = { when_status = 404, error = "no_connection", detail = "resource not found" } }
```

### Custom transforms (extension API)

Register named transforms at startup using `ProxyBuilder`:

```rust
use synapse_proxy::{ProxyBuilder, config::Config};
use synapse_proxy::transform::{RequestTransform, ProxyRequest, TransformError};
use synapse_proxy::context::ResolvedContext;
use async_trait::async_trait;
use std::sync::Arc;

struct MyTransform;

#[async_trait]
impl RequestTransform for MyTransform {
    async fn apply(&self, ctx: &ResolvedContext, req: &mut ProxyRequest) -> Result<(), TransformError> {
        req.set_header("x-custom", ctx.get("tenant").unwrap_or("unknown"));
        Ok(())
    }
}

let router = synapse_proxy::build_router_from_config(
    ProxyBuilder::from_config(config)
        .request_transform("my-transform", Arc::new(MyTransform))
).unwrap();
```

Then reference it in TOML:
```toml
request_steps = [ { transform = "my-transform" } ]
```

## Listeners

Three listeners are served concurrently:

| listener | config key | default | purpose |
|---|---|---|---|
| Data plane | `addr` | `0.0.0.0:8787` | Proxy traffic. |
| Admin | `admin_addr` | `127.0.0.1:8788` | Context push/clear. |
| Metrics | `metrics_addr` | `0.0.0.0:9090` | Prometheus scrape. |

## Endpoints

**Data plane**

- `GET /healthz/liveness` — always 200.
- `GET /healthz/readiness` — 200, or 503 once shutting down (SIGTERM drains in-flight requests).
- Everything else — matched against `routes` and forwarded; `404 {"error":"no_route"}` if none match, `502 {"error":"request_failed"}` if the upstream is unreachable.

**Admin** (`admin_addr`)

- `POST /internal/bind` — push a context overlay. Body: `{"values":{"org":"acme"},"ttl_seconds":3600}`.
- `DELETE /internal/bind` — clear the overlay and revert to the startup baseline.

**Metrics** (`metrics_addr`)

- `GET /metrics` — Prometheus text format.

### Metric names

| name | type | labels | description |
|---|---|---|---|
| `synapse_proxy_requests_total` | counter | `route`, `method`, `status`, `outcome` | Total forwarded requests. |
| `synapse_proxy_request_duration_seconds` | histogram | `route`, `method` | Request duration. |
| `synapse_proxy_upstream_errors_total` | counter | `route`, `reason` | Upstream connection/read errors. |
| `synapse_proxy_transform_errors_total` | counter | `route`, `transform` | Transform pipeline errors. |
