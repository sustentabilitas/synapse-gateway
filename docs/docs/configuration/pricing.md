---
sidebar_position: 4
title: Pricing
description: Reference for pricing.toml, the per-model prices Synapse uses to compute the cost of every request in the cost ledger.
---

`pricing.toml` holds the price of each model your routes use. Synapse multiplies a
request's token counts by these prices to record its cost in the cost ledger.

The gateway reads the file from `SYNAPSE_PRICING_PATH` (default `config/pricing.toml`) at
startup and refuses to start if it is missing or invalid. An empty file is valid: every
request then costs 0.

## Format

Each entry is keyed by `provider:model` and has an `input` and an `output` price in USD per
1,000,000 tokens. Both prices are required.

```toml title="config/pricing.toml"
"vertex:gemini-3.5-flash-lite" = { input = 0.30, output = 2.50 }
"vertex:gemini-3.6-flash" = { input = 1.50, output = 7.50 }
"openai:gpt-4o-mini" = { input = 0.15, output = 0.60 }
"typesafe:jev-latest" = { input = 0.042, output = 0.0 }
```

The same prices written as TOML tables, the form the shipped
[`config/pricing.toml`](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-gateway/config/pricing.toml)
uses:

```toml title="config/pricing.toml"
["vertex:gemini-3.5-flash-lite"]
input = 0.30
output = 2.50

["vertex:gemini-3.6-flash"]
input = 1.50
output = 7.50

["openai:gpt-4o-mini"]
input = 0.15
output = 0.60

["typesafe:jev-latest"]
input = 0.042
output = 0.0
```

## How keys are matched

- `provider` is the leg's provider id: `vertex`, `openai`, `qwen`, `oai_compat` or
  `typesafe`.
- `model` is the leg's `model` exactly as written in `routes.toml`, not the route alias.
  A route with two legs needs two entries.
- Jev routing decisions are priced as `typesafe:<model>`, where `<model>` is the route's
  `jev_router.model` (default `jev-latest`).

## How cost is computed

For each completed request:

```text
cost_usd = (input_tokens × input + output_tokens × output) / 1,000,000
```

A chat model without an entry costs 0 and the request still succeeds, which suits
self-hosted models. An embedding model without an entry is priced at
`SYNAPSE_EMBED_DEFAULT_INPUT_PRICE_PER_MTOK` (default `0.10`) per 1,000,000 input tokens, so
embedding usage is never silently free.

Token counts come from the provider's response. Keep two native Vertex details in mind:

- `input_tokens` includes tokens read from a context cache, and Synapse prices them at the
  full `input` price. Vertex AI bills cached tokens at a discount, so the ledger overstates
  the cost of cached requests.
- `output_tokens` is Vertex AI's `candidatesTokenCount`, which does not include thinking
  tokens. Vertex AI bills thinking tokens as output, so the ledger understates the cost of
  requests that think.

Prices change; check each provider's price list and update the file when they do.
