# synapse-mcp

[![crates.io](https://img.shields.io/crates/v/synapse-mcp.svg)](https://crates.io/crates/synapse-mcp)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](https://github.com/sustentabilitas/synapse-gateway/blob/main/LICENSE)
[![CI](https://github.com/sustentabilitas/synapse-gateway/actions/workflows/ci.yml/badge.svg)](https://github.com/sustentabilitas/synapse-gateway/actions/workflows/ci.yml)

synapse-mcp is an on-demand [Model Context Protocol](https://modelcontextprotocol.io/) (MCP)
gateway library. A client, typically code running in a sandbox, speaks MCP over Streamable
HTTP to `/mcp/{server}` on a loopback listener. The gateway forwards each tool call to the
upstream MCP server registered under that name, over a connection that carries the current
tenant's identity headers, taken from a shared `ContextStore`. The client never sees the
upstream URL and can't choose its own identity.

It is a library, not a program: neither the synapse-proxy nor the synapse-gateway binary
mounts it. Your application serves its two axum routers, typically next to
[synapse-proxy](https://crates.io/crates/synapse-proxy) so that both share one context store.
It is built on [`rmcp`](https://docs.rs/rmcp) 2.2.

**Full documentation:** https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/mcp/overview/

## Install

```bash
cargo add synapse-mcp synapse-context
```

## Example

```rust
use std::{collections::HashMap, sync::Arc, time::Duration};

use synapse_context::ContextStore;
use synapse_mcp::{
    mcp_admin_router, mcp_gateway_router, IdentityHeaderRule, McpGatewayConfig, McpRegistry,
};

async fn serve() -> anyhow::Result<()> {
    let context = Arc::new(ContextStore::new(HashMap::new()));
    context.push(HashMap::from([("org".into(), "my-team".into())]), Some(Duration::from_secs(3600)));

    let registry = Arc::new(McpRegistry::new());
    let config = Arc::new(McpGatewayConfig {
        inject: vec![IdentityHeaderRule {
            context_key: "org".into(),
            header: "x-org-id".into(),
            required: true,
        }],
    });

    let admin = mcp_admin_router(registry.clone());
    let gateway = mcp_gateway_router(registry, context, config, None);

    let admin_listener = tokio::net::TcpListener::bind("127.0.0.1:8788").await?;
    let mcp_listener = tokio::net::TcpListener::bind("127.0.0.1:8789").await?;
    tokio::try_join!(axum::serve(admin_listener, admin), axum::serve(mcp_listener, gateway))?;
    Ok(())
}
```

Register an upstream server on the admin listener; clients then call it at
`http://127.0.0.1:8789/mcp/platform`, and every call carries `x-org-id: my-team`:

```bash
curl -s -X POST localhost:8788/internal/mcp/servers \
  -H 'Content-Type: application/json' \
  -d '{"name":"platform","url":"http://platform-mcp:8080/mcp","ttl_seconds":3600}'
```

## Learn more

- [Mount it next to the proxy](https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/mcp/overview/#mount-it-next-to-the-proxy)
- [Registration](https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/mcp/registration/)
- [Identity injection](https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/mcp/identity-injection/)
- [Security](https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/mcp/security/)
- [Roadmap](https://synapse-gateway.readthedocs.io/en/latest/docs/synapse-family/mcp/roadmap/)
- API reference on [docs.rs](https://docs.rs/synapse-mcp)

## License

Licensed under the **GNU Affero General Public License v3.0** (AGPL-3.0). See **[LICENSE](https://github.com/sustentabilitas/synapse-gateway/blob/main/LICENSE)**.
