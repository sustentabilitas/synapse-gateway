---
slug: introducing-synapse
title: "Introducing Synapse: an LLM gateway that keeps native power"
authors: [rajwilkhu]
tags: [release, vertex, routing]
description: Synapse is an open-source Rust LLM gateway that speaks the OpenAI API to your clients and keeps Vertex AI's native features, Jev routing and per-tenant cost accounting behind it.
---

Synapse is an open-source LLM gateway written in Rust. Your clients send standard OpenAI
`POST /v1/chat/completions` requests, and Synapse routes each one through a config-driven
fallback chain of providers, records what it cost, and hands back an OpenAI-shaped response.

What makes it different is what it refuses to throw away. Vertex AI's context caching,
Cloud Storage media and strict response schemas survive the trip, and a model can decide how
much model each request deserves. This post explains why we built it, how it is put together
and where it is going.

<!-- truncate -->

## We tried not to write one

We started where most teams start: put a generic OpenAI-compatible proxy in front of every
provider and move on. That approach reaches Gemini through an OpenAI-shaped adapter, and the
adapter is where the features we depend on disappear. There is no OpenAI field for a Vertex
`cachedContents` resource, for a `gs://` video, or for a strict `responseSchema`, so a
translation layer either drops them or never learns about them.

We wanted multi-provider routing and fallback, and we wanted Vertex's native features, not
one or the other. So Synapse keeps a dedicated native lane for Vertex, alongside the
OpenAI-compatible lane that serves everything else, and keeps the codebase small enough that
the routing, fallback, ledger and metrics code is yours to read, run and embed.

## Three lanes, one endpoint

Every chat request goes to the same endpoint. The request body alone decides which of three
lanes serves it, so a client opts into native features by adding an extension block, with no
second API to learn. The [architecture overview](/docs/overview/architecture/) walks through
the whole request flow.

- **The standard lane** calls providers through the [`genai`](https://crates.io/crates/genai)
  crate. OpenAI, Qwen (DashScope) and self-hosted vLLM, Ollama or TGI through the
  `oai_compat` provider can all appear in one fallback chain, and so can Vertex, without its
  native-only features.
- **The native Vertex lane** takes any request whose `vertex` block carries
  `cached_content`, `response_schema`, `thinking_config` or a `gs://` media URI. Synapse
  translates the OpenAI messages to Vertex's format and calls Vertex AI's
  `:streamGenerateContent` endpoint directly, buffering the stream for clients that did not
  ask for one. Only the route's `vertex` legs take part; if a route has none, the request fails
  with `native_feature_unsupported` instead of silently losing the features. See the
  [native Vertex guide](/docs/guides/native-vertex/).
- **The Jev lane** sends typed questions to TypeSafe System One (Jev), which returns
  structured decisions instead of free text. It can also judge and then extract in a single
  call. See the [Jev lane guide](/docs/guides/jev-lane/).

A native request looks like any other chat completion, plus a `vertex` block:

```json
{
  "model": "gemini-flash",
  "messages": [{ "role": "user", "content": "Summarise the video." }],
  "vertex": {
    "media_uris": ["gs://cloud-samples-data/video/animals.mp4"],
    "response_schema": { "type": "object", "properties": { "summary": { "type": "string" } } }
  }
}
```

Here `gemini-flash` is a route alias for `gemini-3.5-flash-lite` on Vertex, as in the
[quickstart](/docs/get-started/quickstart/).

## Routing by difficulty

Lanes decide *how* a request reaches a provider. Routes decide *which* legs it tries. A
static route is an ordered list of legs, tried until one succeeds. A `strategy = "jev"` route
instead declares difficulty tiers, described by the kind of work they suit. For each request,
the Jev router asks Jev how demanding the conversation is and whether it needs step-by-step
reasoning, then serves it from the matching tier with a matching reasoning effort. The
response says what happened in `x-synapse-routing`, `x-synapse-tier` and related headers.

A companion post, [Jev routing explained](/blog/jev-routing-explained/), covers the
decision in detail, and the [Jev router guide](/docs/guides/jev-router/) is the reference.

## The baseline, not an add-on

A few things we consider table stakes come with every deployment:

- **Real streaming.** Synapse always streams from upstream, so `stream: true` clients get
  token-by-token server-sent events. Non-streaming clients get the buffered result and, on
  the standard lane, keep the full fallback chain. See
  [Streaming and tool calling](/docs/guides/streaming-and-tools/).
- **Tool calling on both lanes.** On the native Vertex lane, `tool_choice` is honoured
  through Vertex `toolConfig`.
- **A cost ledger you own.** Every request is attributed to a tenant from the
  `x-synapse-tenant` header and priced from your `pricing.toml` into SQLite or Postgres, with
  optional fan-out to Google Cloud Pub/Sub and AWS SNS. See the
  [cost ledger guide](/docs/guides/cost-ledger/).
- **Metrics.** OpenTelemetry `synapse_*` metrics, served in Prometheus format on port 9090
  and optionally pushed over OTLP. The
  [OpenTelemetry metrics post](/blog/opentelemetry-metrics/) explains the pipeline.
- **Input guardrails.** Named scanner policies that block or observe requests before they
  reach a provider. See [Guardrails policy](/docs/configuration/guardrails-policy/).

Synapse is licensed under AGPL-3.0, and none of this sits behind a paid tier.

## Run it, or embed it

You can run Synapse as a single binary or as the `sustentabilitas/synapse-gateway` Docker
image, with the API on port 8080 and metrics on port 9090. The
[quickstart](/docs/get-started/quickstart/) gets a gateway answering in a few minutes.

If your service is written in Rust, you can skip the extra process entirely. Depend on the
`synapse-gateway` crate with `default-features = false`, build a `Gateway` in code, and call
`Gateway::chat()` in-process, with the same routing, fallback and ledger behaviour as the
binary and no HTTP hop. See
[Embedding Synapse as a library](/docs/guides/embedding-as-library/).

## The family

Synapse is a Cargo workspace, and the gateway has siblings that grew out of the same needs:

- **[`synapse-proxy`](/docs/synapse-family/proxy/overview/)** is a config-driven reverse-proxy
  sidecar. It routes by path prefix and stamps a bound identity, such as a tenant, on every
  forwarded request, so a sandboxed workload cannot choose its own.
- **[`synapse-a2a`](/docs/synapse-family/a2a/overview/)** is an agent-to-agent (A2A) registry
  with admin registration and public discovery, served by the gateway binary.
- **[`synapse-mcp`](/docs/synapse-family/mcp/overview/)** is an on-demand MCP gateway library
  that routes Streamable HTTP tool calls per server and injects the current tenant's identity.
- **`synapse-context`** is the shared context store behind the proxy and the MCP gateway.

[Workspace crates](/docs/internals/workspace-crates/) shows how they depend on each other.

## What's next

We would rather you learn the gaps here than in production. Inbound authentication, rate
limiting and dynamic route reloading are planned; today, run Synapse behind your own API
gateway, ingress or service mesh. The gateway records metrics but emits no trace spans, and
chat legs get one attempt each, with no retries or circuit breakers. The
[limitations and roadmap](/docs/reference/limitations-roadmap/) page lists every one of
these, with links to the details.

The code is on [GitHub](https://github.com/sustentabilitas/synapse-gateway), and
[contributions](/docs/contributing/) are welcome. Try the quickstart, and tell us what breaks.
