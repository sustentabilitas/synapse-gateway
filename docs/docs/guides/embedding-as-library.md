---
sidebar_position: 9
title: Embedding Synapse as a library
description: Depend on the synapse-gateway crate without its HTTP server and call Gateway::chat, chat_stream and embed in-process from your Rust service.
---

Embed Synapse when your service is written in Rust and you want routing, fallback, Jev
decisions and cost accounting without running and operating a separate gateway process. You
build a `Gateway` in code and call it directly: no HTTP hop, no extra deployment, and the
same routing behaviour as the binary. Run the binary instead when several services in
different languages share one gateway, or when you want the gateway's metrics and ledger in
one place.

The API reference is on [docs.rs](https://docs.rs/synapse-gateway).

## Add the dependency

Turn off the default features, which add the HTTP server and the SQLite ledger:

```toml
[dependencies]
synapse-gateway = { version = "0.5", default-features = false }
anyhow = "1"
futures = "0.3"
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The library crate is named `synapse`, so you import it as `synapse::…`. Add
`ledger-sqlite`, `ledger-postgres`, `ledger-pubsub` or `ledger-sns` to `features` for a
ledger backend; see [Installation](../get-started/installation.md#cargo).

## Build a gateway

`Gateway::builder()` takes the route table, the provider catalog, the pricing table and a
ledger handle, all required, plus optional parts. This example serves the quickstart's
`chat` route:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use synapse::config::vertex_project_from_env;
use synapse::gateway::{ChatOutcome, Gateway, RequestCtx};
use synapse::ledger::{LedgerHandle, NoopLedger};
use synapse::pricing::PricingTable;
use synapse::providers::vertex_auth::VertexAuth;
use synapse::providers::Catalog;
use synapse::routing::request::ChatRequest;
use synapse::routing::stream::StreamItem;
use synapse::routing::table::RouteTable;
use synapse::vertex_native::VertexNativeProvider;

const ROUTES: &str = r#"
[routes."chat"]
legs = [
  { provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" },
  { provider = "openai", model = "gpt-4o-mini" },
]
"#;

const PRICING: &str = r#"
"vertex:gemini-3.5-flash-lite" = { input = 0.30, output = 2.50 }
"openai:gpt-4o-mini" = { input = 0.15, output = 0.60 }
"#;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let env: HashMap<String, String> = std::env::vars().collect();
    let timeout = Duration::from_secs(120);
    let routes = RouteTable::from_toml_str(ROUTES)?;
    let catalog = Catalog::build(&env, &routes.referenced_providers(), timeout)?;

    let gateway = Gateway::builder()
        .routes(routes)
        .catalog(catalog)
        .pricing(PricingTable::from_toml_str(PRICING)?)
        .ledger(LedgerHandle::spawn(Arc::new(NoopLedger), 1024))
        .vertex_native(vertex_project_from_env(&env).map(|project| {
            VertexNativeProvider::new(
                Arc::new(VertexAuth::from_adc()),
                project,
                "global".into(),
                timeout,
                None,
            )
        }))
        .default_tenant("my-service")
        .build()?;

    let ctx = RequestCtx {
        tenant: Some("my-team".into()),
        ..Default::default()
    };
    let req: ChatRequest = serde_json::from_value(serde_json::json!({
        "model": "chat",
        "messages": [{ "role": "user", "content": "Say hello in one word." }],
    }))?;

    let (outcome, routing) = gateway.chat_routed(req.clone(), &ctx).await?;
    if let ChatOutcome::Plain(c) = outcome {
        println!("{} answered: {}", c.model, c.content);
        println!("tokens: {} in, {} out", c.input_tokens, c.output_tokens);
    }
    println!("routing: {:?}", routing.headers());

    let mut stream = gateway.chat_stream(req, &ctx).await?;
    println!("streaming from {}", stream.model());
    while let Some(item) = stream.next().await {
        match item {
            Ok(StreamItem::Delta(text)) => print!("{text}"),
            Ok(_) => {}
            Err(e) => eprintln!("stream failed: {e}"),
        }
    }
    Ok(())
}
```

Things to know about the pieces:

- **`RouteTable` and `PricingTable`** parse the same TOML as `routes.toml` and
  `pricing.toml`. Read them from files with `std::fs::read_to_string` in a real service.
- **`Catalog::build`** creates a client for every provider the routes reference, reading
  credentials from the map you pass, and fails if one is missing, like strict validation in
  the binary. It does not read the process environment by itself.
- **`LedgerHandle::spawn`** starts the background writer on the current Tokio runtime.
  `NoopLedger` discards usage; pass a store such as `synapse::ledger::sqlite::SqliteLedger`
  (feature `ledger-sqlite`) or a `FanoutLedger` of several stores to keep it. The second
  argument is the queue size.
- **`vertex_native`** enables the native Vertex lane. The last argument of
  `VertexNativeProvider::new` overrides the API host, for tests; `None` derives it from each
  leg's region.

## Chat

`Gateway::chat(req, &ctx)` runs a request through guardrails, route planning and the
fallback chain, records usage, and returns a `ChatOutcome`:

- `ChatOutcome::Plain(Completion)` for a normal completion, with `provider`, `model`,
  `content`, `tool_calls`, `finish_reason`, `input_tokens` and `output_tokens`.
- `ChatOutcome::Hybrid(HybridOutcome)` for a [hybrid extraction](jev-lane.md#hybrid-extraction),
  with `answers`, `survivors`, `degraded` and the per-candidate `extractions`.

`ChatRequest` is the OpenAI request body, including the `vertex` and `jev` blocks, so the
easiest way to build one is to deserialise JSON, as above. `RequestCtx` carries the
attribution the HTTP headers carry in the binary; see
[Tenant attribution](tenant-attribution.md#in-an-embedded-gateway). Set its `request_id` to
correlate ledger rows with your own ids.

Errors are `synapse::error::GatewayError`: `UnknownModel`, `BadRequest`,
`NativeFeatureUnsupported`, `ContentBlocked`, `AllLegsFailed` (with each leg's failure),
`UpstreamTimeout`, `Upstream` and `AllCircuitsOpen`, the same cases the HTTP API maps to
status codes.

## Read the routing report

`Gateway::chat_routed(req, &ctx)` returns `(ChatOutcome, RoutingReport)`. The report says
how the request was routed, which matters on [Jev router](jev-router.md) routes:

| Field | Meaning |
|---|---|
| `mode` | `RoutingMode::Jev`, `Static` or `StaticOverride`. |
| `tier` | The tier that served, if the route has tiers. |
| `tier_decided` | The tier Jev chose, only when a different tier served. |
| `effort` | The serving leg's effort, or `"client"`. |
| `degraded` | `"timeout"`, `"error"`, `"low_confidence"` or `"jev_unavailable"` when Jev's decision was not used. |

`RoutingReport::headers()` returns the same values as the `x-synapse-*` response headers
the binary sends.

## Stream

`Gateway::chat_stream(req, &ctx)` commits to the first leg that produces output and returns
a `GuardedStream`, a `Stream` of `Result<StreamItem, LegError>` items:

- `StreamItem::Delta` carries text, `StreamItem::ToolCallDelta` a tool call fragment, and
  `StreamItem::Done` the finish reason and token counts.
- `stream.model()` is the serving model, and `stream.routing()` the `RoutingReport`.
- Usage is recorded exactly once, when the stream is fully consumed or dropped, so dropping
  it early still writes a ledger row.

The [streaming rules](streaming-and-tools.md#fallback-while-streaming) apply: no fallback
after the first item.

## Embeddings

To serve embedding aliases, give the builder the embedding table and one embedder per
provider, then call `Gateway::embed(req, ctx)`, which takes the context by value:

```rust
use synapse::embeddings::vertex::VertexEmbedder;
use synapse::embeddings::{EmbeddingInput, EmbeddingRequest};
use synapse::routing::embeddings::EmbeddingRouteTable;

let gateway = Gateway::builder()
    // ... routes, catalog, pricing and ledger as above
    .embed_routes(EmbeddingRouteTable::from_toml_str(ROUTES)?)
    .embedder(
        "vertex",
        Arc::new(VertexEmbedder::new(
            Arc::new(VertexAuth::from_adc()),
            project,
            "global".into(),
            timeout,
        )),
    )
    .build()?;

let resp = gateway
    .embed(
        EmbeddingRequest {
            input: EmbeddingInput::Many(vec!["first chunk".into(), "second chunk".into()]),
            model: "embed".into(),
            dimensions: None,
        },
        RequestCtx {
            tenant: Some("my-team".into()),
            ..Default::default()
        },
    )
    .await?;
```

`EmbeddingRouteTable` reads the `[embeddings."<alias>"]` tables from the same TOML as your
routes; see [Embeddings](embeddings.md).

## What the builder leaves out

The binary wires up several parts from its environment that a builder-made gateway only
has if you add them:

| Part | Default | Add it with |
|---|---|---|
| Native Vertex lane | Off: native requests fail with `400`. | `.vertex_native(Some(VertexNativeProvider::new(...)))` |
| Jev lane and Jev router | Off: Jev lane requests fail with `400`, and `jev` routes skip the decision and start at `default_tier`, reporting `jev_unavailable`. | `.jev_native(Some(JevNativeProvider::new(api_key, None, timeout)))` |
| Guardrails | Off. | `.guard(GuardEngine::from_config(&GuardrailsConfig::from_toml_str(..)?)?)` |
| Metrics | Recorded nowhere. | `.metrics(Arc::new(GatewayMetrics::new(&meter)))` with a meter from your OpenTelemetry `MeterProvider` |
| AI task types | `simple`, unless the `RequestCtx` sets `ai_task_type`. | `.ai_task_types(AiTaskTypeTable::from_toml_str(..)?)` |
| Timeouts | 120 s first chunk, 60 s idle. | `.timeouts(StreamTimeouts { first_chunk, idle })` |
| Default tenant | `unattributed`. | `.default_tenant("...")` |
| Embeddings | No aliases. | `.embed_routes(...)` and `.embedder(...)`, as above. |

With the `server` feature, `synapse::telemetry::install` builds the same Prometheus and OTLP
exporters the binary uses and returns the `GatewayMetrics` to pass to `.metrics(...)`. Keep
the `MetricsExporter` it returns alive for the life of the process.
