---
sidebar_position: 2
title: Workspace crates
description: The five crates in the Synapse Cargo workspace, what each one does, and how they depend on each other.
---

Synapse is one Cargo workspace with five crates under `crates/`. All five are published to
crates.io, use the 2021 edition, and are licensed MPL-2.0. Versions on this page are the
ones in each crate's `Cargo.toml` on `main`.

| Crate | Version | Library name | Binary | Docker image |
|---|---|---|---|---|
| [`synapse-gateway`](#synapse-gateway) | 2.0.0 | `synapse` | `synapse-gateway` | `sustentabilitas/synapse-gateway` |
| [`synapse-proxy`](#synapse-proxy) | 1.0.0 | `synapse_proxy` | `synapse-proxy` | `sustentabilitas/synapse-proxy` |
| [`synapse-context`](#synapse-context) | 0.1.1 | `synapse_context` | — | — |
| [`synapse-a2a`](#synapse-a2a) | 0.2.2 | `synapse_a2a` | — | — |
| [`synapse-mcp`](#synapse-mcp) | 0.1.4 | `synapse_mcp` | — | — |

## Dependencies between the crates

```text
synapse-gateway ──(feature "server")──► synapse-a2a

synapse-proxy ──► synapse-context ◄── synapse-mcp
```

The gateway and the proxy don't depend on each other. `synapse-context` has no workspace
dependencies, and neither does `synapse-a2a`. Nothing in the workspace depends on
`synapse-mcp`: applications that build on the proxy mount it themselves.

## synapse-gateway

The LLM gateway, and the subject of most of this documentation. The library, imported as
`synapse`, contains the `Gateway` type and everything behind it: routing, the three lanes,
guardrails, pricing, the ledger and the metrics. The `synapse-gateway` binary wraps it in the
HTTP server described in the [HTTP API](../reference/http-api.md).

Cargo features:

| Feature | Default | Adds |
|---|---|---|
| `server` | Yes | The axum HTTP server, the Prometheus and OTLP exporters, and `synapse-a2a`. |
| `ledger-sqlite` | Yes | The SQLite ledger sink. |
| `ledger-postgres` | No | The Postgres ledger sink. |
| `ledger-pubsub` | No | The Google Cloud Pub/Sub ledger sink. |
| `ledger-sns` | No | The AWS SNS ledger sink. |

To embed the gateway without the HTTP server, turn off the default features; see
[Embedding Synapse as a library](../guides/embedding-as-library.md).
[Request pipeline](./request-pipeline.md) walks through the source.

- crates.io: [synapse-gateway](https://crates.io/crates/synapse-gateway)
- docs.rs: [synapse-gateway](https://docs.rs/synapse-gateway)

## synapse-proxy

A config-driven reverse-proxy sidecar. It forwards requests to upstreams by longest matching
path prefix, injects static or context-derived headers and body fields, runs request and
response transforms, and passes streaming responses through. It serves three listeners: the
data plane (`addr`, default `0.0.0.0:8787`), an admin listener (`admin_addr`, default
`127.0.0.1:8788`) where `POST /internal/bind` and `DELETE /internal/bind` push and clear the
bound context, and a metrics listener (`metrics_addr`, default `0.0.0.0:9090`).

The library exposes `ProxyBuilder`, for registering custom transforms, and re-exports
`synapse-context`'s types as `synapse_proxy::context`. Its metrics are in the
[metrics catalogue](../reference/metrics-catalogue.md#synapse-proxy), and
[Docker](../deployment/docker.md#run-the-proxy) shows how to run the image.

- crates.io: [synapse-proxy](https://crates.io/crates/synapse-proxy)
- docs.rs: [synapse-proxy](https://docs.rs/synapse-proxy)

## synapse-context

The bound-context store that `synapse-proxy` and `synapse-mcp` share. A `ContextStore` holds a
permanent **base** map of keys to values, built once at startup (the proxy merges its static
configuration with environment variables, the environment winning), and at most one
**overlay**, pushed at runtime with an optional time to live. `push` replaces the overlay,
`clear` removes it, and `resolve` returns a `ResolvedContext`: the base with the overlay's keys
on top while the overlay is live. An expired overlay is dropped the next time the store is
resolved, so the context reverts to the base without a background task. Only one binding is
active at a time, which is why `synapse-mcp` supports a single identity per process. The store
lives in its own crate so that `synapse-proxy` and `synapse-mcp` can share the same type
without depending on each other, and `synapse-proxy` re-exports it so existing import paths
keep working.

- crates.io: [synapse-context](https://crates.io/crates/synapse-context)
- docs.rs: [synapse-context](https://docs.rs/synapse-context)

## synapse-a2a

An in-memory registry of agent-to-agent (A2A) agents. It provides the admin router
(`POST /internal/a2a/agents`, `DELETE /internal/a2a/agents/{id}`), the public router (the
catalogue, agent cards and resolve), and the `a2a.toml` seeding that fetches each agent's card
at startup. Registration is insert-only: the first registration of an id wins. Entries can
carry a time to live.

The gateway binary creates one registry at startup and merges both routers into its API
listener; the [HTTP API](../reference/http-api.md#a2a-agent-registry) documents the endpoints
and [Configuration keys](../reference/configuration-keys.md#a2atoml) the seed file.

- crates.io: [synapse-a2a](https://crates.io/crates/synapse-a2a)
- docs.rs: [synapse-a2a](https://docs.rs/synapse-a2a)

## synapse-mcp

An on-demand Model Context Protocol (MCP) gateway, built on `rmcp` 2.2. Clients speak MCP over
Streamable HTTP to `/mcp/{server}`, and the gateway forwards each call to the upstream MCP
server registered under that name, adding identity headers from a shared `ContextStore`
according to its configured rules. A required context key that is missing fails the call before
any upstream is contacted. Upstream servers are registered at runtime through the admin router
(`POST /internal/mcp/servers`, `DELETE /internal/mcp/servers/{name}`), optionally with a time
to live.

The crate provides routers and a metrics struct, not a binary: an application mounts
`mcp_gateway_router` and `mcp_admin_router` next to its own listeners and passes in the
`ContextStore` it shares with the proxy library. Its metrics are in the
[metrics catalogue](../reference/metrics-catalogue.md#synapse-mcp) and its unsupported features
in [Limitations](../reference/limitations-roadmap.md#synapse-mcp).

- crates.io: [synapse-mcp](https://crates.io/crates/synapse-mcp)
- docs.rs: [synapse-mcp](https://docs.rs/synapse-mcp)
