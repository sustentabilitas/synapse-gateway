# synapse-proxy

A config-driven reverse-proxy sidecar. It forwards incoming requests to upstreams
by longest matching `path_prefix`, optionally stripping the prefix, injecting
static headers, and streaming the response back.

## Configuration

Set `SYNAPSE_PROXY_CONFIG_PATH` (default `synapse-proxy.toml`); `SYNAPSE_PROXY_ADDR`
overrides the listen address.

```toml
addr = "0.0.0.0:8787"

[[routes]]
path_prefix = "/v1/llm"
upstream = "http://synapse-gateway:8080"
strip_prefix = true        # /v1/llm/chat -> {upstream}/chat
[routes.headers]
x-forwarded-by = "synapse-proxy"
```

## Endpoints

- `GET /healthz/liveness` — always 200.
- `GET /healthz/readiness` — 200, or 503 once shutting down (SIGTERM drains in-flight requests).
- Everything else — matched against `routes` and forwarded; `404 {"error":"no_route"}` if none match,
  `502 {"error":"request_failed"}` if the upstream is unreachable.
