---
sidebar_position: 2
title: Proxy configuration
description: The synapse-proxy TOML file, its environment variables, and the upstream timeouts and retries.
---

The proxy reads one TOML file at startup and a few environment variables. Nothing is reloaded
while it runs: change the file and restart the proxy.

## The configuration file

The proxy loads the file named by `SYNAPSE_PROXY_CONFIG_PATH`, by default
`synapse-proxy.toml` in the working directory. The Docker image's working directory is `/app`
and it ships an example at `/app/synapse-proxy.toml`; mount your own file over it. A missing
or unreadable file stops the proxy at startup.

| Key | Type | Default | Description |
|---|---|---|---|
| `addr` | string | `0.0.0.0:8787` | Data-plane listen address. `SYNAPSE_PROXY_ADDR` overrides it. |
| `admin_addr` | string | `127.0.0.1:8788` | Admin listen address, for `/internal/bind`. |
| `metrics_addr` | string | `0.0.0.0:9090` | Metrics listen address, for `/metrics`. |
| `[context]` | table | empty | Where the context comes from; see [Context sources](./context-and-transforms.md#context-sources). |
| `[[routes]]` | array of tables | none | The routes, described below. With no routes, every request gets `404`. |

Unknown keys are ignored, so a misspelt key silently has no effect. Check a new file against
the tables on this page.

### Routes

Each `[[routes]]` table forwards one path prefix to one upstream:

| Key | Type | Required | Default | Description |
|---|---|---|---|---|
| `path_prefix` | string | Yes | — | Requests whose path starts with this string match the route. The longest match wins. |
| `upstream` | string | Yes | — | Base URL to forward to: scheme, host and an optional path prefix. |
| `name` | string | No | `path_prefix` | Label used in metrics and logs. |
| `strip_prefix` | bool | No | `false` | Remove `path_prefix` from the path before appending it to `upstream`. |
| `methods` | array of strings | No | any | Only match these HTTP methods, for example `["POST"]`. Case-insensitive. |
| `headers` | table | No | none | Static headers set on every forwarded request, before `request_steps` run. |
| `require_context` | array of strings | No | none | Context keys that must be bound, or the request gets `503`. |
| `request_steps` | array | No | none | Transforms applied to the request, in order. |
| `response_steps` | array | No | none | Transforms applied to the upstream response, in order. |

The steps are described in [Context and transforms](./context-and-transforms.md). A step that
is invalid, such as an `inject` with both `header` and `body`, or a `transform` name that
wasn't registered, stops the proxy at startup with an error naming the route.

### Example

```toml
addr = "0.0.0.0:8787"
admin_addr = "127.0.0.1:8788"
metrics_addr = "0.0.0.0:9090"

[context]
static = { user = "_default" }
env = { org = "TENANT_ORG_ID", workspace = "TENANT_WORKSPACE_ID" }

[[routes]]
name = "orders"
path_prefix = "/v1/orders"
upstream = "http://orders:8080"
strip_prefix = true
require_context = ["org", "workspace"]
headers = { X-Api-Version = "2" }
request_steps = [
  { inject = { header = "X-Tenant-Id",    from_context = "org" } },
  { inject = { header = "X-Workspace-Id", from_context = "workspace" } },
  { inject = { header = "X-User-Id",      from_context = "user" } },
]

[[routes]]
name = "integrations"
path_prefix = "/v1/integrations/call"
upstream = "http://integrations:8080/call"
strip_prefix = true
methods = ["POST"]
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

With this file, `GET /v1/orders/42?expand=items` is forwarded to
`http://orders:8080/42?expand=items` with the tenant, workspace, user and API version headers
set. `POST /v1/integrations/call` is forwarded to `http://integrations:8080/call` with its
JSON body wrapped as `{"request": <original body>, "org": "...", "workspace": "..."}`.

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `SYNAPSE_PROXY_CONFIG_PATH` | `synapse-proxy.toml` | Path to the configuration file. |
| `SYNAPSE_PROXY_ADDR` | `addr` from the file | Overrides the data-plane listen address. Ignored when empty. |
| `SYNAPSE_PROXY_UPSTREAM_CONNECT_TIMEOUT_SECS` | `10` | Time allowed to connect to an upstream. |
| `SYNAPSE_PROXY_UPSTREAM_TIMEOUT_SECS` | `120` | Time allowed for a whole upstream request, including reading a streamed response body. |
| `SYNAPSE_PROXY_UPSTREAM_SEND_RETRIES` | `2` | Retries after a failed send; `0` disables retries. |
| `SYNAPSE_PROXY_UPSTREAM_RETRY_BACKOFF_MS` | `200` | Delay before the first retry; it doubles for each further retry. |
| `RUST_LOG` | `info` | Log filter, in `tracing` `EnvFilter` syntax. |

The environment variables that the `[context]` table names are read at startup too; see
[Context sources](./context-and-transforms.md#context-sources). A value that isn't a valid
number falls back to the default.

## Timeouts and retries

The upstream timeout covers the whole exchange, from connecting until the last byte of the
response body. A streamed response that runs longer than
`SYNAPSE_PROXY_UPSTREAM_TIMEOUT_SECS` is cut off, so raise it for long-lived streams.

When a request fails before the proxy gets a response from the upstream, it is retried:

- **Connection failures** are retried for every method, because the request never reached
  the upstream.
- **Other send failures**, including timeouts, are retried only for idempotent methods:
  `GET`, `HEAD`, `OPTIONS`, `TRACE`, `PUT` and `DELETE`. A `POST` or `PATCH` that may have
  reached the upstream is not sent twice.

Retries wait 200 ms, then 400 ms, and so on with the defaults. When the retries run out, the
client gets `502` with `"error": "request_failed"`. A response from the upstream is never
retried, whatever its status: a `503` from the upstream is passed to the client. Each retry
and each final failure is logged as a warning and counted in the
[metrics](./metrics.md).
