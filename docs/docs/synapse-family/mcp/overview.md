---
sidebar_position: 1
title: synapse-mcp
description: synapse-mcp is a library that routes MCP tool calls over Streamable HTTP to upstream MCP servers registered on demand, injecting tenant identity from a shared context store.
---

`synapse-mcp` is an on-demand [Model Context Protocol](https://modelcontextprotocol.io/)
(MCP) gateway. A client, typically code running in a sandbox, speaks MCP over Streamable HTTP
to `/mcp/{server}` on a loopback listener. The gateway forwards each tool call to the
upstream MCP server registered under that name, over its own connection that carries the
identity headers of the current tenant. The client never sees the upstream URL and can't set
its own identity.

It is built on [`rmcp`](https://docs.rs/rmcp) 2.2, the official Rust MCP SDK.

## When to use it

Use it with [`synapse-proxy`](../proxy/overview.md) when untrusted code must call MCP servers
on behalf of one tenant: your control plane binds the tenant and registers the servers the
session may use, and the sandbox reaches only those servers, only as that tenant.

## What it is

`synapse-mcp` is a library, not a program. It provides two axum routers that your
application mounts:

- `mcp_gateway_router` serves `/mcp/{server}`, the MCP endpoint clients connect to.
- `mcp_admin_router` serves `POST /internal/mcp/servers` and
  `DELETE /internal/mcp/servers/{name}`, which register upstream servers.

Neither the proxy binary nor the gateway binary mounts them. There is no default port and
no configuration file: your application chooses the listeners and builds the configuration.

```text
sandbox code ──MCP / Streamable HTTP──▶ your app, 127.0.0.1  /mcp/<server>
                                          │ registry.resolve(<server>)   → upstream URL, or error
                                          │ ContextStore.resolve()       → identity, or fail closed
                                          │ upstream rmcp client with the identity headers set
                                          ▼
                                       upstream MCP server
```

The gateway exposes tools only. `tools/list` and `tools/call` are forwarded to the upstream;
resources, prompts and the other MCP capabilities are not offered.

## Mount it next to the proxy

The gateway reads identity from a `ContextStore` (from `synapse-context`). Share the proxy's
store, and an identity bound through the proxy's `POST /internal/bind` applies to MCP calls
straight away. This sketch builds the proxy from its configuration file, adds the MCP admin
routes to the proxy's admin listener, and serves the MCP gateway on its own loopback
listener:

```rust
use std::sync::Arc;

use synapse_mcp::{
    mcp_admin_router, mcp_gateway_router, GatewayMetrics, IdentityHeaderRule, McpGatewayConfig,
    McpRegistry,
};
use synapse_proxy::{admin::admin_router, config::Config, metrics::Metrics, ProxyBuilder};

async fn serve_mcp() -> anyhow::Result<()> {
    let built = ProxyBuilder::from_config(Config::load()?).build()?;
    let (metrics, _registry) = Metrics::new()?;

    let registry = Arc::new(McpRegistry::new());
    let config = Arc::new(McpGatewayConfig {
        inject: vec![
            IdentityHeaderRule { context_key: "org".into(), header: "x-org-id".into(), required: true },
            IdentityHeaderRule { context_key: "user".into(), header: "x-user-id".into(), required: true },
        ],
    });

    let admin = admin_router(built.context.clone()).merge(mcp_admin_router(registry.clone()));
    let gateway = mcp_gateway_router(
        registry,
        built.context.clone(),
        config,
        Some(GatewayMetrics::new(&metrics.meter())),
    );

    let admin_listener = tokio::net::TcpListener::bind(&built.admin_addr).await?;
    let mcp_listener = tokio::net::TcpListener::bind("127.0.0.1:8789").await?;
    tokio::try_join!(
        axum::serve(admin_listener, admin),
        axum::serve(mcp_listener, gateway),
    )?;
    Ok(())
}
```

A real service also serves the proxy's data plane and metrics listeners, and exports the
`Registry` returned by `Metrics::new`; the proxy's
[`main.rs`](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-proxy/src/main.rs)
shows how. Pass `None` instead of `GatewayMetrics` to record no metrics.

## Learn more

- [Registration](./registration.md): add and remove upstream MCP servers.
- [Identity injection](./identity-injection.md): which headers are sent upstream, and when a
  call fails closed.
- [Security](./security.md): the loopback and `Host` checks, and what errors reveal.
- [Roadmap](./roadmap.md): what isn't supported yet.
- The gateway's metrics are in the
  [metrics catalogue](../../reference/metrics-catalogue.md#synapse-mcp), and the API is on
  [docs.rs](https://docs.rs/synapse-mcp).
