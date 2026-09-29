---
sidebar_position: 1
title: synapse-proxy
description: synapse-proxy is a config-driven reverse-proxy sidecar that routes by path prefix, injects context-derived headers and body fields, and streams responses back.
---

`synapse-proxy` is a config-driven reverse-proxy sidecar. It forwards each request to an
upstream chosen by the longest matching path prefix, can strip that prefix, injects headers
and JSON body fields from a per-process **context** (such as the tenant a sandbox runs for),
runs request and response transforms, and streams the upstream response back. It is a
separate binary and Docker image from the gateway, and the two don't depend on each other.

## When to use it

Use the proxy when a workload must call internal services with an identity it should not be
able to choose. Your control plane binds the identity once, through the proxy's admin
listener, and the proxy stamps it on every forwarded request, removing any value the caller
tried to send. Typical cases:

- A code sandbox or agent runtime that calls platform APIs on behalf of one tenant.
- A sidecar that adds fixed headers, such as a user id or an API version, to every call.
- Wrapping requests in the envelope an upstream expects, or normalising its error bodies.

The proxy doesn't authenticate callers, and it doesn't balance load across several
upstreams: each route has one upstream URL.

## How it works

```text
caller ──► :8787 data plane ──► match route (longest path_prefix, methods)
                                  │  check require_context        → 503 if unbound
                                  │  request steps (inject, wrap, custom)
                                  ▼
                                upstream ──► response steps (error_remap, custom) ──► caller

control plane ──► 127.0.0.1:8788 admin   POST/DELETE /internal/bind  (sets the context)
Prometheus    ──► :9090 metrics          GET /metrics
```

## Quick start

Save this as `synapse-proxy.toml`. It forwards `/httpbin/...` to the public
[httpbin](https://httpbin.org) service and stamps an `X-Team` header from the context:

```toml
addr = "127.0.0.1:8787"
admin_addr = "127.0.0.1:8788"
metrics_addr = "127.0.0.1:9090"

[context]
static = { team = "my-team" }

[[routes]]
name = "httpbin"
path_prefix = "/httpbin"
upstream = "https://httpbin.org"
strip_prefix = true
require_context = ["team"]
request_steps = [
  { inject = { header = "X-Team", from_context = "team" } },
]
```

Run it from a checkout of the repository:

```bash
SYNAPSE_PROXY_CONFIG_PATH=synapse-proxy.toml cargo run --release -p synapse-proxy
```

The metrics port is `9090`, the same as the gateway's; if you run both on one machine, change
`metrics_addr`. Then send a request, rebind the context, and send it again:

```bash
curl -s localhost:8787/httpbin/headers -H 'X-Team: spoofed'
# "X-Team": "my-team"   (the caller's value is overwritten)

curl -s -X POST localhost:8788/internal/bind \
  -H 'Content-Type: application/json' \
  -d '{"values":{"team":"other-team"},"ttl_seconds":60}'

curl -s localhost:8787/httpbin/headers
# "X-Team": "other-team"   (for 60 seconds, then back to my-team)
```

To run the Docker image instead, see [Run the proxy](../../deployment/docker.md#run-the-proxy).

## Next steps

- [Configuration](./configuration.md): the file, environment variables, timeouts and retries.
- [Context and transforms](./context-and-transforms.md): where context comes from and what
  each transform does.
- [Listeners and endpoints](./listeners-and-endpoints.md): what each listener serves, and
  every error the proxy returns.
- [Metrics](./metrics.md): what to scrape and alert on.
