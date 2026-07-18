# synapse-mcp

An MCP (Model Context Protocol) gateway for the `synapse-proxy` sandbox broker. Sandbox code speaks MCP over Streamable HTTP to a loopback endpoint; the gateway routes each call to a backend MCP server **registered on-demand** for that session, injecting the session's tenant identity from the broker's `ContextStore`.

Built on [`rmcp`](https://docs.rs/rmcp) 2.2 (server + client + streamable-HTTP transports).

## How it fits

`synapse-proxy` mounts this crate as a 4th loopback listener (default `127.0.0.1:8789`) alongside its data / admin / metrics planes, sharing the same `Arc<ContextStore>` — so an identity binding pushed via the proxy's `POST /internal/bind` is immediately visible to the gateway.

```
sandbox code ──MCP/Streamable-HTTP──▶ synapse-proxy :8789  /mcp/<server>
                                         │  registry.resolve(<server>)  → upstream URL (or reject)
                                         │  ContextStore.resolve()       → org/workspace/user (or fail-closed)
                                         │  upstream rmcp client w/ x-org-id/x-workspace-id/x-user-id baked in
                                         ▼
                                      backend MCP server
```

## Registration (on-demand)

Backend MCP servers are registered per sandbox session on the proxy's **admin** listener (merged from this crate):

- `POST /internal/mcp/servers` — `{ "name": "platform", "url": "http://backend/mcp", "ttl_seconds": 3600 }` → register/replace (hot-swap).
- `DELETE /internal/mcp/servers/{name}` — deregister.

Servers may also be seeded statically via `synapse-proxy.toml`:

```toml
mcp_addr = "127.0.0.1:8789"
[[mcp_upstreams]]
name = "platform"
url = "http://backend/mcp"
ttl_seconds = 3600
```

Registration is TTL-scoped (mirroring the identity binding's TTL) so a session's servers expire with its identity.

## Identity injection

On every forwarded request the gateway resolves the `ContextStore` overlay and sets `x-org-id` / `x-workspace-id` / `x-user-id` from the bound `org` / `workspace` / `user` keys. Client-supplied copies of those headers are never forwarded — the bound identity always wins.

Because rmcp fixes a transport's HTTP headers at construction (**per-connection**, not per-call), the gateway caches one upstream client per `(server, identity)` and rebuilds it when either the identity overlay **or** the registered URL changes.

Requests are **fail-closed**: an unbound / partially-bound overlay is rejected before any upstream is contacted; unknown/expired server names error without a network call.

## Security

- Loopback-only listener.
- **DNS-rebinding protection** is on by default — rmcp's `StreamableHttpServerConfig::default()` allows only `localhost` / `127.0.0.1` / `::1` as `Host` and returns `403` otherwise (asserted in tests).
- Upstream URLs / transport errors are logged internally but not surfaced in sandbox-facing MCP errors.

## Not yet (future work)

- **Tool aggregation** across servers (one merged tool surface) — v1 is transparent per-server routing (`/mcp/<server>`).
- **SSE back-compat** for upstreams that only speak the legacy transport.
- Multiple concurrent identity bindings (v1 has a single active overlay, matching `ContextStore`).
