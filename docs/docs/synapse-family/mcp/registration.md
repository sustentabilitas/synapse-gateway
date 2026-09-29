---
sidebar_position: 2
title: Registration
description: Register and remove the upstream MCP servers that synapse-mcp routes to, with optional expiry.
---

The gateway only routes to servers that are registered. Your control plane registers the
servers a session may use when the session starts, usually with the same time to live as the
session's identity binding, so both expire together.

## Admin endpoints

`mcp_admin_router` serves two endpoints. Mount it on a listener that only your control plane
can reach, such as the proxy's loopback admin listener. They have no authentication.

### `POST /internal/mcp/servers`

```bash
curl -s -X POST localhost:8788/internal/mcp/servers \
  -H 'Content-Type: application/json' \
  -d '{"name":"platform","url":"http://platform-mcp:8080/mcp","ttl_seconds":3600}'
```

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | Yes | The name clients use in `/mcp/{name}`. |
| `url` | string | Yes | The upstream server's Streamable HTTP MCP endpoint. |
| `ttl_seconds` | integer | No | Seconds until the registration expires. Omit it for no expiry. |

Returns `204 No Content`. Registering a name that already exists replaces its URL and expiry.
This is a hot swap: the next call to that name connects to the new URL and closes the
connection to the old one.

### `DELETE /internal/mcp/servers/{name}`

Removes the registration. Returns `204 No Content` whether or not the name was registered.

## Expiry

A registration with `ttl_seconds` stops resolving once that many seconds have passed. Calls to
an expired or unknown name fail without any network call, with the MCP error
`unknown or expired mcp server '<name>'`.

The gateway keeps one open connection per registered server. Connections to servers that have
been removed or have expired are closed during the next call that reaches an upstream, to any
server.

## Register in code

There is no seed file. To register servers at startup, call the registry directly before you
serve the routers:

```rust
use std::sync::Arc;
use std::time::Duration;

use synapse_mcp::McpRegistry;

let registry = Arc::new(McpRegistry::new());
registry.register("platform".into(), "http://platform-mcp:8080/mcp".into(), Some(Duration::from_secs(3600)));
```

`register`, `deregister` and `resolve` behave like the endpoints above. Pass the same
`Arc<McpRegistry>` to `mcp_admin_router` and `mcp_gateway_router`.
