---
sidebar_position: 3
title: Context and transforms
description: How synapse-proxy builds its per-request context, and the inject, wrap and error_remap transforms that use it, plus custom transforms in Rust.
---

The **context** is a set of string keys and values, such as `org = "acme"`, that the proxy
can stamp on forwarded requests. Transforms are the steps that do the stamping, reshape
request bodies and normalise upstream errors.

## Context sources

The context has two layers.

The **base** is built once at startup from the `[context]` table:

- `static` sets literal values: `static = { user = "_default" }`.
- `env` maps a context key to the environment variable to read it from:
  `env = { org = "TENANT_ORG_ID" }`. When a key is in both, the environment variable wins.
  A variable that is unset or empty leaves the key out, or at its `static` value.

The **overlay** is pushed at runtime through the admin listener. It sits on top of the base:
its keys win, and base keys it doesn't mention stay visible.

```bash
curl -s -X POST localhost:8788/internal/bind \
  -H 'Content-Type: application/json' \
  -d '{"values":{"org":"acme","workspace":"team-a"},"ttl_seconds":3600}'
```

- There is one overlay at a time. Each `POST` replaces the previous overlay completely, so
  send every key you want bound, not just the ones that changed.
- With `ttl_seconds`, the overlay expires after that many seconds and the context reverts to
  the base. Without it, the overlay lasts until it is replaced or cleared.
- `DELETE /internal/bind` removes the overlay straight away.

The context is resolved for each request, so a new binding applies to the next request. The
store is the `ContextStore` from the `synapse-context` crate; see
[Workspace crates](../../internals/workspace-crates.md#synapse-context).

## Requiring context

List the keys a route can't do without in `require_context`. If any is missing, from both
the base and the overlay, the proxy answers without contacting the upstream:

```http
HTTP/1.1 503 Service Unavailable

{"error":"request_failed","detail":"context not bound"}
```

## Request steps

`request_steps` run in order, after any static `headers` on the route. A step that fails
stops the request; see [Errors](./listeners-and-endpoints.md#errors).

### inject

Sets one header or one JSON body field, from a context key or a constant:

```toml
{ inject = { header = "X-Tenant-Id", from_context = "org" } }
{ inject = { header = "X-User-Id",   const = "_default" } }
{ inject = { body = "tenant",        from_context = "org" } }
{ inject = { body = "params.context.user", from_context = "user" } }
```

- Set exactly one of `header` and `body`, and exactly one of `from_context` and `const`.
- `body` is a dotted path. Missing objects along the path are created, and a value in the
  way that isn't an object is replaced.
- `const` can be any TOML value. In a body it keeps its type; in a header, a string is sent
  as is and anything else as its JSON text.
- An injected value overwrites whatever the caller sent in that header or field.

When a `from_context` key is not bound:

- **Header targets fail safe.** The proxy removes the header, so a value the caller sent
  can't pass through in place of the missing identity.
- **Body targets are left alone.** The caller's value, if any, is forwarded. Use
  `require_context` to stop these requests.

:::warning
Any context key you inject as identity, such as a tenant or user id, should also be listed in
the route's `require_context`. That is the only guard for body targets, and it makes a
missing binding an explicit `503` instead of a request without identity.
:::

### wrap

Nests the whole JSON request body under a key, then injects sibling fields:

```toml
{ wrap = { under = "request", inject = [
    { body = "org",       from_context = "org" },
    { body = "workspace", from_context = "workspace" },
] } }
```

A body of `{"method":"GET","path":"/p"}` is forwarded as
`{"request":{"method":"GET","path":"/p"},"org":"acme","workspace":"team-a"}`. The entries in
`inject` take the same keys as the `inject` step.

### Body handling

A body step (`inject` with `body`, or `wrap`) needs a JSON body. An empty body counts as `{}`;
anything else that isn't valid JSON is rejected with `400` and `"error": "invalid_body"`.
Requests that no body step touches are forwarded byte for byte, JSON or not.

After a body step, the proxy re-serialises the JSON and drops the caller's `Content-Length`
and `Transfer-Encoding` headers, so the upstream receives a length that matches the new
body.

## Response steps

`response_steps` run in order once the upstream has answered.

### error_remap

When the upstream status equals `when_status`, replaces the response body with a normalised
error object. The status code is kept:

```toml
{ error_remap = { when_status = 401, error = "auth_expired" } }
{ error_remap = { when_status = 404, error = "no_connection", detail = "resource not found" } }
```

The first line turns any `401` body into `{"error":"auth_expired"}`; the second adds
`"detail"`. A remapped response is sent as JSON, without the upstream's headers. Responses
that no step remaps are streamed to the caller unchanged.

## Custom transforms

To do something the built-in steps can't, write a transform in Rust and run the proxy as a
library. Implement `RequestTransform` or `ResponseTransform` (both use `async-trait`), and
register it by name on a `ProxyBuilder`:

```rust
use std::sync::Arc;

use async_trait::async_trait;
use synapse_proxy::config::Config;
use synapse_proxy::context::ResolvedContext;
use synapse_proxy::transform::{ProxyRequest, RequestTransform, TransformError};
use synapse_proxy::ProxyBuilder;

struct TenantTag;

#[async_trait]
impl RequestTransform for TenantTag {
    async fn apply(&self, ctx: &ResolvedContext, req: &mut ProxyRequest) -> Result<(), TransformError> {
        req.set_header("x-tenant-tag", ctx.get("org").unwrap_or("unknown"));
        Ok(())
    }
}

fn data_plane(config: Config) -> anyhow::Result<axum::Router> {
    synapse_proxy::build_router_from_config(
        ProxyBuilder::from_config(config).request_transform("tenant-tag", Arc::new(TenantTag)),
    )
}
```

Then use it in the configuration:

```toml
request_steps = [ { transform = "tenant-tag" } ]
```

- `ProxyRequest` gives you the method, path, query and headers, `set_header`,
  `remove_header` and `body_json_mut`. `ProxyResponse` gives you the status and headers and
  `replace_body`.
- Return `TransformError::Reject { status, error, detail }` to answer the caller with that
  status and `{"error", "detail"}` body, or `TransformError::Internal(message)` for a `500`.
- `build_router_from_config` builds the data plane only, with metrics that aren't exported.
  To serve the admin and metrics listeners as well, call `ProxyBuilder::build` and assemble
  the routers the way the binary does in
  [`main.rs`](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-proxy/src/main.rs).

The API is documented on [docs.rs](https://docs.rs/synapse-proxy).
