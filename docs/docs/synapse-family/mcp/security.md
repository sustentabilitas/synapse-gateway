---
sidebar_position: 4
title: MCP security
description: The protections synapse-mcp relies on, from loopback listeners and Host checks to fail-closed identity and what errors reveal.
---

`synapse-mcp` sits on a trust boundary: sandbox code on one side, internal MCP servers and
tenant identity on the other. These are the protections it provides, and the ones you must
provide when you mount it.

## Listen on loopback only

Bind the gateway router to `127.0.0.1` or `::1`, so only processes in the same host, container
or pod can reach it. The crate doesn't choose the address; your application does.

## Host header check

The gateway keeps `rmcp`'s DNS-rebinding protection switched on. A request whose `Host` header
isn't `localhost`, `127.0.0.1` or `::1`, on any port, is rejected with `403` before it reaches
the gateway.
This stops a web page from reaching the gateway through a hostname that resolves to loopback.
It also means clients must address the gateway by one of those names; a Kubernetes Service or
pod IP won't work.

## Identity fails closed

A call whose `required` identity keys aren't all bound is refused before any upstream is
contacted, and client-supplied identity headers are never forwarded. See
[Identity injection](./identity-injection.md).

## Unauthenticated control surfaces

The admin routes (`/internal/mcp/servers`) and the proxy's `/internal/bind` have no
authentication. Whoever can call them can point a server name at any URL, or bind any
identity. Make sure the code you are isolating can't reach them: if sandbox code shares a
loopback interface with the admin listener, it can rebind its own identity.

## What errors reveal

- Calls to unknown or expired servers fail with `unknown or expired mcp server '<name>'`.
- Failures to connect to an upstream are logged as warnings with the details, including the
  URL, and the client gets only `upstream MCP server unavailable`.
- Each `tools/list` and `tools/call`, including connecting to the upstream, is bounded by 30
  seconds. A call that runs longer fails with `upstream MCP call timed out`.
- Errors returned by the upstream on an established connection, for `tools/list` or
  `tools/call`, are passed to the client as the error message text. Make sure your upstream
  servers don't put secrets in error messages.

## Related

- [Security](../../operating/security.md) for the gateway.
- [Listeners and endpoints](../proxy/listeners-and-endpoints.md) for the proxy's admin
  listener.
