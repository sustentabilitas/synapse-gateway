---
sidebar_position: 1
title: What is Synapse?
description: Synapse is an open-source Rust LLM gateway that is OpenAI-compatible on the outside and keeps native Vertex AI and Jev routing on the inside.
---

## What is Synapse?

Synapse is an open-source LLM gateway written in Rust. Your clients send standard OpenAI
`POST /v1/chat/completions` requests; Synapse routes each one through a config-driven
fallback chain of providers and records what it cost. Unlike generic OpenAI-compatible
proxies, it keeps a native Vertex AI lane and a TypeSafe Jev lane, so Vertex-only
features and per-request model routing survive the trip. You run it as a single binary
or Docker image, or embed it in a Rust service as a library.

Synapse is a Cargo workspace. The crates are:

- **`synapse-gateway`**: the LLM gateway, and the subject of most of this documentation.
- **`synapse-proxy`**: a config-driven reverse-proxy sidecar with path-prefix routing,
  context injection, request and response transforms, and streaming passthrough.
- **`synapse-a2a`**: an agent-to-agent (A2A) registry with admin registration and public
  discovery endpoints, served by the gateway binary.
- **`synapse-mcp`**: an on-demand MCP gateway that routes Streamable HTTP traffic per
  server and injects per-session tenant identity.
- **`synapse-context`**: the shared context store (a static base plus a TTL overlay) used
  by `synapse-proxy` and `synapse-mcp`.

## Why is Synapse different?

We tried not to write another gateway. We evaluated
[`litellm-rs`](https://github.com/majiayu000/litellm-rs) and the general "put an
OpenAI-compatible proxy in front of everything" approach first. It would have cost us the
one thing we could not give up: native Vertex AI. Synapse exists because of that trade-off.

- **Native Vertex, not the lowest common denominator.** Most OpenAI-compatible gateways
  reach Gemini through a generic OpenAI-shaped adapter, which throws away context caching
  (`cachedContent`), Cloud Storage (`gs://`) media URIs and strict `responseSchema`
  constrained decoding. Synapse keeps a dedicated native Vertex lane that calls
  `:generateContent` and `:streamGenerateContent` directly, while every other request rides
  the standard OpenAI-compatible lane. You get multi-provider routing and Vertex's native
  features, not one or the other.
- **Routing decisions made by a model.** The Jev lane sends typed questions to TypeSafe
  System One (Jev) and returns structured decisions. The Jev router asks Jev how demanding
  each request is, then picks a model tier and reasoning effort for it.
- **Small and owned, not a framework.** Synapse is one Rust binary, or a library crate you
  call in-process with `Gateway::chat()`. Because the routing, fallback, ledger and
  observability code is ours, features other gateways lacked were straightforward to add:
  a per-tenant cost ledger with multi-sink fan-out, and OpenTelemetry `gen_ai.*` spans and
  metrics on every request.
- **The essentials are the baseline.** Synapse always streams from the upstream provider,
  so `stream: true` clients get token-by-token server-sent events and non-streaming clients
  keep fallback across the whole chain. Tool calling works on both the standard and native
  Vertex lanes, and existing OpenAI SDKs work unchanged.

## When to use Synapse

Use Synapse when:

- You depend on Vertex-only features such as context caching, `gs://` media URIs or strict
  `responseSchema`, and still want an OpenAI-compatible API with fallback to other providers.
- You want each request routed to the right model and reasoning effort for its difficulty
  (the Jev router), instead of hard-coding one model per use case.
- You need per-tenant cost accounting: every request is attributed to a tenant and priced
  into a ledger you own.
- You want to embed an LLM gateway inside a Rust service rather than run a separate process.

Synapse is not a good fit when:

- You need inbound authentication or rate limiting today. Synapse has neither yet; run it
  behind your own API gateway, ingress or service mesh.
- You want a hosted SaaS. Synapse is self-hosted software.

## Key features

Native capabilities:

- **Native Vertex lane**: `cachedContent`, `gs://` media URIs, strict `responseSchema` and
  `thinking_config`, sent directly to `:generateContent` and `:streamGenerateContent`. See
  [Native Vertex lane](architecture.md#native-vertex-lane).
- **Native tool calling**: tools work on both lanes; on the native Vertex lane,
  `tool_choice` is honoured through Vertex `toolConfig`.
- **Jev lane**: TypeSafe System One typed decisions, plus hybrid judge-then-extract in one
  call. See [Jev lane](architecture.md#jev-lane).
- **Jev router**: per-request tier and reasoning effort, mapped to Vertex `thinkingBudget`
  on native Vertex legs and reported in `x-synapse-routing` and `x-synapse-tier` response
  headers.
- **Real streaming**: Synapse always streams from upstream; non-streaming clients get the
  buffered result and keep the full fallback chain.
- **Embeddable**: run the `synapse-gateway` binary, or depend on the library crate and call
  `Gateway::chat()` in-process.

Compatibility and operations:

- **OpenAI-compatible API**: existing OpenAI SDKs work unchanged, including
  `POST /v1/embeddings`.
- **Multi-provider fallback**: Vertex AI, OpenAI, Qwen (DashScope), and self-hosted vLLM,
  Ollama or TGI through the `oai_compat` provider.
- **Per-tenant cost ledger**: token usage and cost per request, written to SQLite or
  Postgres and optionally fanned out to Google Cloud Pub/Sub and AWS SNS.
- **Observability**: OpenTelemetry metrics served in Prometheus format (and optionally
  pushed over OTLP), plus `gen_ai.*` tracing spans.
- **Input guardrails**: named scanner policies (prompt injection, secrets, PII and more)
  that block or observe requests before they reach a provider.
