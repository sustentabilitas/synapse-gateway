---
sidebar_position: 2
title: Route requests by difficulty with Jev
description: Build a strategy = "jev" route with three tiers, send an easy and a hard prompt, and read which tier and reasoning effort Synapse chose for each.
---

In this tutorial you add an `auto` route that picks a model and a reasoning effort for each
request. Instead of a fixed list of legs, the route declares three difficulty tiers. Before
serving a request, Synapse asks TypeSafe Jev how demanding it is and whether it needs
step-by-step reasoning, then serves it from the matching tier. You send an easy and a hard
prompt and read the decision from the response headers, the cost ledger and the metrics.

## Before you begin

- Complete the [Quickstart](../quickstart.md). This tutorial reuses its
  `synapse-quickstart` directory and credentials.
- Get a TypeSafe API key. Jev makes the routing decision, so a `jev` route needs
  `TYPESAFE_API_KEY`.

## Add a tiered route

Replace `config/routes.toml` with this file. It keeps the quickstart's two routes and adds
`auto`:

```toml title="config/routes.toml"
[routes."gemini-flash"]
legs = [
  { provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" },
]

[routes."chat"]
legs = [
  { provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" },
  { provider = "openai", model = "gpt-4o-mini" },
]

[routes."auto"]
strategy = "jev"

[routes."auto".jev_router]
default_tier = "moderate"
timeout_ms = 400

[[routes."auto".tiers]]
name = "trivial"
description = "Greetings, chit-chat, one-line lookups or rewrites"
effort = "none"
legs = [{ provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" }]

[[routes."auto".tiers]]
name = "moderate"
description = "Everyday Q&A, summarising, simple extraction or code edits"
effort = "low"
legs = [{ provider = "vertex", model = "gemini-3.6-flash", region = "global" }]

[[routes."auto".tiers]]
name = "hard"
description = "Multi-step analysis, maths or proofs, non-trivial code or debugging"
effort = "medium"
legs = [{ provider = "vertex", model = "gemini-3.1-pro-preview", region = "global" }]
```

What the `auto` route declares:

- `strategy = "jev"` replaces `legs` with tiers, listed from easiest to hardest. A route has
  2 to 10 tiers.
- Jev scores each request against the tier `description`s, so they describe the work, never
  the model. Tier `name`s are sent back in response headers, so they must be unique,
  printable ASCII.
- `effort` is the reasoning effort the tier's legs run with: `none`, `minimal`, `low`,
  `medium`, `high`, `xhigh` or `max`. On the standard lane Synapse passes it to the `genai`
  crate, which sends it as `reasoning_effort` to OpenAI-style providers and as
  `thinkingLevel` to these Gemini 3 models. `none` sends nothing, so the model's default
  applies. See [Effort](../../configuration/routes.md#effort) for the full mapping.
- `default_tier` serves when Jev is unsure, slow or unavailable. `timeout_ms` bounds the
  decision call; 400 ms is the default.
- Tier legs can use any provider except `typesafe`: Jev decides, it is not a candidate.

[Jev routes](../../configuration/routes.md#jev-routes) in the routes reference lists every
key and its default.

Add prices for the new models and for Jev's decisions to `config/pricing.toml`, so the
ledger can cost them (see [Pricing](../../configuration/pricing.md)):

```toml title="config/pricing.toml"
"vertex:gemini-3.5-flash-lite" = { input = 0.30, output = 2.50 }
"vertex:gemini-3.6-flash" = { input = 1.50, output = 7.50 }
"vertex:gemini-3.1-pro-preview" = { input = 2.00, output = 12.00 }
"openai:gpt-4o-mini" = { input = 0.15, output = 0.60 }
"typesafe:jev-latest" = { input = 0.042, output = 0.0 }
```

## Restart the gateway with a TypeSafe key

Synapse reads its configuration at startup, so stop the gateway and start it again with
`TYPESAFE_API_KEY`. With Docker:

```bash
docker run --rm -p 8080:8080 -p 9090:9090 \
  -e VERTEX_PROJECT_ID=my-gcp-project \
  -e OPENAI_API_KEY=sk-... \
  -e TYPESAFE_API_KEY=... \
  -e GOOGLE_APPLICATION_CREDENTIALS=/secrets/sa.json \
  -e SYNAPSE_LEDGER_SQLITE_DSN="sqlite:///app/data/synapse.db?mode=rwc" \
  -v "$(pwd)/sa.json:/secrets/sa.json:ro" \
  -v "$(pwd)/config:/app/config" \
  -v "$(pwd)/data:/app/data" \
  sustentabilitas/synapse-gateway
```

With Cargo, `export TYPESAFE_API_KEY=...` and run the quickstart's `synapse-gateway`
command again.

A `jev` route references the `typesafe` provider, so under the default `strict` provider
validation the gateway refuses to start without `TYPESAFE_API_KEY`. Under
`SYNAPSE_PROVIDER_VALIDATION=lenient` it starts, but `auto` downgrades to a static route
that always tries `moderate` first, then `hard`, then `trivial`.

## Send an easy prompt

Send a greeting to `auto` and print only Synapse's routing headers:

```bash
curl -s -D - -o /dev/null http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "auto",
    "messages": [{"role": "user", "content": "Hi there!"}]
  }' | grep -i '^x-synapse'
```

Jev rates a greeting as trivial, so you typically see:

```text
x-synapse-routing: jev
x-synapse-tier: trivial
x-synapse-reasoning-effort: none
```

- `x-synapse-routing: jev` says Jev made the decision.
- `x-synapse-tier` is the tier that served the request.
- `x-synapse-reasoning-effort` is the effort that tier's leg ran with.

Remove `-D - -o /dev/null` and the `grep` to see the response body: a normal
`chat.completion` whose `model` is `gemini-3.5-flash-lite`.

## Send a hard prompt

Now ask for a proof:

```bash
curl -s -D - -o /dev/null http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "auto",
    "messages": [{"role": "user", "content": "Prove that there are infinitely many primes of the form 4k + 3."}]
  }' | grep -i '^x-synapse'
```

Typical headers:

```text
x-synapse-routing: jev
x-synapse-tier: hard
x-synapse-reasoning-effort: high
```

The request went to the `hard` tier. Its configured effort is `medium`, but Jev also judged
that the prompt needs step-by-step reasoning, so Synapse raised the effort one step, to
`high`. It raises the effort when Jev's reasoning score reaches `reasoning_threshold`
(default 0.7).

Jev's ratings depend on the prompt, so your tiers can differ. Try your own prompts and
watch the headers change.

## Override the decision

A client can set its own effort. Add `"reasoning_effort": "low"` to the request body: Jev
still picks the tier, but the leg uses your effort and the header reads
`x-synapse-reasoning-effort: client`.

A client can also skip the decision. Add `"routing_strategy": "static"` and Synapse does
not call Jev: the request goes to `default_tier` with that tier's effort, and the headers
read:

```text
x-synapse-routing: static-override
x-synapse-tier: moderate
x-synapse-reasoning-effort: low
```

On a route without tiers, `"routing_strategy": "jev"` returns `400`.

## See the decisions in the ledger

Each decision Jev makes is written to the ledger as its own row, sharing the `request_id`
of the chat request it routed:

```bash
sqlite3 data/synapse.db \
  "SELECT request_id, provider, model, lane, input_tokens, cost_usd
   FROM usage_events WHERE route = 'auto' ORDER BY id;"
```

For each Jev-routed request there are two rows with the same `request_id`: the decision,
with provider `typesafe`, model `jev-latest` and lane `jev`, and the chat completion, with
the serving tier's Vertex model and lane `standard`. The `static-override` request has only
the chat row, because Jev was not called.

## See the decisions in the metrics

```bash
curl -s http://localhost:9090/metrics | grep synapse_routing
```

`synapse_routing_decisions_total` counts decisions by `route`, the decided `tier` and
`outcome` (`decided`, `static_override`, `low_confidence`, `timeout` or `error`).
`synapse_routing_decision_duration_seconds` records how long each Jev call took.

## When Jev cannot decide

A Jev problem never fails the request. If Jev times out, returns an error or answers with
confidence below `min_confidence` (default 0.5), the request goes to `default_tier` and the
response carries `x-synapse-routing-degraded` with the reason: `timeout`, `error`,
`low_confidence` or `jev_unavailable`.

If the chosen tier's legs fail, Synapse tries the harder tiers first, then the easier ones.
When a different tier ends up serving, `x-synapse-tier-decided` names the tier Jev chose.
[Failure behaviour](../../guides/jev-router.md#failure-behaviour) in the Jev router guide
covers each reason and how to monitor it.

## Next steps

- [Fall back across providers](fallback-across-providers.md) to see how a route recovers
  when a leg fails.
- Look up [tiers and strategies](../../overview/concepts.md#tier) in Concepts.
- Tune the decision with the [`jev_router` keys](../../configuration/routes.md#jev-routes).
