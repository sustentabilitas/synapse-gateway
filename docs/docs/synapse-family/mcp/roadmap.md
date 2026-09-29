---
sidebar_position: 5
title: MCP roadmap
description: What synapse-mcp doesn't support yet.
---

The first version of `synapse-mcp` routes each MCP server separately and forwards tools only.
Not yet supported:

- **Tool aggregation across servers.** There is no merged tool surface. Each server is reached
  at its own `/mcp/<server>` path, and a client that needs several servers connects to each
  path.
- **SSE back-compat.** Upstream servers must speak Streamable HTTP; servers that support only
  the legacy HTTP+SSE transport can't be registered.
- **Several concurrent identity bindings.** One identity is active per process at a time,
  matching `synapse-context`'s `ContextStore`.
- **Resources, prompts and other capabilities.** Only `tools/list` and `tools/call` are
  forwarded.
- **Static configuration.** There is no seed file and no binary; servers are registered
  through the admin routes or in code. See [Registration](./registration.md).

The same gaps are listed with the rest of Synapse's in
[Limitations and roadmap](../../reference/limitations-roadmap.md#synapse-mcp).
