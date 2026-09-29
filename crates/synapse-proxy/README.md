# synapse-proxy

[![crates.io](https://img.shields.io/crates/v/synapse-proxy.svg)](https://crates.io/crates/synapse-proxy)
[![Docker Hub](https://img.shields.io/docker/v/sustentabilitas/synapse-proxy?logo=docker&label=docker)](https://hub.docker.com/r/sustentabilitas/synapse-proxy)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL--2.0-brightgreen.svg)](https://github.com/sustentabilitas/synapse-gateway/blob/main/LICENSE)
[![CI](https://github.com/sustentabilitas/synapse-gateway/actions/workflows/ci.yml/badge.svg)](https://github.com/sustentabilitas/synapse-gateway/actions/workflows/ci.yml)

synapse-proxy is a config-driven reverse-proxy sidecar. It forwards each request to an
upstream chosen by the longest matching path prefix, can strip that prefix, injects headers
and JSON body fields from a per-process context (such as the tenant a sandbox runs for), runs
request and response transforms, and streams the upstream response back. Your control plane
binds the context once through the proxy's loopback admin listener, and the proxy stamps it on
every forwarded request, replacing any value the caller tried to send.

**Full documentation:** https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/proxy/overview/

## Install

```bash
# Docker image (linux/amd64)
docker pull sustentabilitas/synapse-proxy

# Binary
cargo install synapse-proxy
```

To run the image, see [Run the proxy](https://synapse-gateway.readthedocs.io/en/latest/docs/deployment/docker/#run-the-proxy).

## Example

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

Start the proxy, send a request, rebind the context, and send it again:

```bash
SYNAPSE_PROXY_CONFIG_PATH=synapse-proxy.toml synapse-proxy

curl -s localhost:8787/httpbin/headers -H 'X-Team: spoofed'
# "X-Team": "my-team"   (the caller's value is overwritten)

curl -s -X POST localhost:8788/internal/bind \
  -H 'Content-Type: application/json' \
  -d '{"values":{"team":"other-team"},"ttl_seconds":60}'

curl -s localhost:8787/httpbin/headers
# "X-Team": "other-team"   (for 60 seconds, then back to my-team)
```

## Learn more

- [Configuration](https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/proxy/configuration/): the file, environment variables, timeouts and retries
- [Context and transforms](https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/proxy/context-and-transforms/), including custom transforms with `ProxyBuilder`
- [Listeners and endpoints](https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/proxy/listeners-and-endpoints/)
- [Metrics](https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/proxy/metrics/)

## License

Licensed under the **Mozilla Public License 2.0** (MPL-2.0), which welcomes commercial use and asks that changes to Synapse's own files are shared back. See **[LICENSE](https://github.com/sustentabilitas/synapse-gateway/blob/main/LICENSE)** and the [licence note](https://github.com/sustentabilitas/synapse-gateway#license).
