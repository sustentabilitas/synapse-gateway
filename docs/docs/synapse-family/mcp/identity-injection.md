---
sidebar_position: 3
title: Identity injection
description: How synapse-mcp turns the bound context into headers on upstream MCP connections, and when it fails closed.
---

On every call, the gateway reads the current context from the shared `ContextStore` and turns
it into headers on its connection to the upstream MCP server. You decide which context keys
become which headers.

## Rules

`McpGatewayConfig` holds a list of rules. Each rule maps one context key to one header:

```rust
use synapse_mcp::{IdentityHeaderRule, McpGatewayConfig};

let config = McpGatewayConfig {
    inject: vec![
        IdentityHeaderRule { context_key: "org".into(), header: "x-org-id".into(), required: true },
        IdentityHeaderRule { context_key: "workspace".into(), header: "x-workspace-id".into(), required: true },
        IdentityHeaderRule { context_key: "user".into(), header: "x-user-id".into(), required: false },
    ],
};
```

Both types implement `serde::Deserialize`, so you can load the rules from your own
configuration file. In TOML, the same rules are:

```toml
[[inject]]
context_key = "org"
header = "x-org-id"
required = true

[[inject]]
context_key = "workspace"
header = "x-workspace-id"
required = true

[[inject]]
context_key = "user"
header = "x-user-id"
```

`required` defaults to `false`. The crate has no built-in idea of identity: with an empty
`inject` list, calls are forwarded with no identity headers at all.

## What happens on a call

For each `tools/list` or `tools/call`, before any network call:

1. The server name from `/mcp/{server}` is looked up in the registry. An unknown or expired
   name fails the call.
2. Every rule is applied to the resolved context:
   - a bound key sets its header;
   - a missing key on a `required` rule **fails the call closed**, with the MCP error
     `context not bound: missing identity key '<key>'`, and counts in
     `broker_identity_injection_failures_total`;
   - a missing key on an optional rule leaves its header out.
3. The call is sent on a connection to the upstream that carries exactly those headers.

Headers from the client's own request are never forwarded. The gateway opens its own
connection to the upstream, so a client that sends `x-org-id` itself can't change the
identity the upstream sees.

## One connection per identity

`rmcp` fixes a connection's HTTP headers when the connection is made; they can't change per
call. So the gateway keeps one upstream connection per server and identity, and makes a new
one when either changes:

- When the bound identity changes, through a new binding or an expired one, the next call
  opens a new connection with the new headers and closes the old one.
- When a server's URL is re-registered, the next call connects to the new URL.

`ContextStore` holds one binding at a time, so the gateway serves one identity at a time per
process. Run one process per concurrent tenant; see [Roadmap](./roadmap.md).

## Choosing rules

Mark every header the upstream relies on for authorisation or tenancy as `required`. An
optional rule suits headers the upstream can do without, such as a user id for audit logs
that falls back to a service identity.
