---
sidebar_position: 3
title: Jev router
description: How a strategy = "jev" route picks a model tier and reasoning effort for each request, what it reports, and what happens when Jev cannot decide.
---

Use the Jev router when one route serves requests of very different difficulty, such as a
chat assistant that gets both greetings and debugging sessions, and you don't want to pay for
your strongest model on every one. Instead of a fixed chain of legs, a `strategy = "jev"`
route declares difficulty tiers. For each request, Synapse asks TypeSafe Jev how demanding
it is and serves it from the matching tier with a matching reasoning effort. If you already
know which model each use case needs, a static route per use case is simpler and adds no
decision latency.

[Jev routes](../configuration/routes.md#jev-routes) lists the configuration keys and their
defaults; the [Jev tiers tutorial](../get-started/tutorials/jev-tiers.md) builds a
three-tier route step by step.

## How a decision is made

Before serving a request, Synapse sends Jev a bounded summary of the conversation:

- the latest user message, up to 16,000 characters (longer messages keep their beginning and
  end);
- the system prompt, up to 2,000 characters;
- as much recent history as fits in a budget of about 24,000 characters in total, each
  message cut to 2,000 characters;
- whether the request carries tools and whether it carries images or media.

Jev answers two questions about it:

1. **Difficulty**: a score against your tier descriptions, in order. Synapse rounds the score
   to the nearest tier.
2. **Needs reasoning**: the probability that a good reply needs step-by-step reasoning, such
   as maths, logic, planning or debugging. At or above `reasoning_threshold` (default 0.7),
   the tier's effort is raised one step.

If Jev's confidence in the difficulty score is below `min_confidence` (default 0.5), the
request goes to `default_tier` instead; the reasoning answer can still raise the effort. The
decision call is bounded by `timeout_ms` (default 400 ms), which adds up to that much
latency to every request on the route.

## Write good tiers

- **Describe the work, never the model.** Jev scores the request against the descriptions,
  so "Multi-step analysis, maths or proofs, non-trivial code or debugging" works;
  "Gemini Pro" does not.
- **Order tiers from easiest to hardest.** The order is what the difficulty score indexes,
  and it drives [tier fallback](#tier-fallback).
- **Keep tiers distinct.** Overlapping descriptions make the score ambiguous, so prefer a few
  clearly different tiers to many similar ones. A route allows 2 to 10.
- **Pick `default_tier` for safety.** It serves whenever Jev cannot decide, so choose a tier
  that handles most requests acceptably, usually a middle one.

## Effort

Each tier sets a reasoning `effort`: `none`, `minimal`, `low`, `medium`, `high`, `xhigh` or
`max`. When the reasoning answer reaches the threshold, the effort goes one step up that
list, stopping at `max`. The tier's legs receive the effort in the form their lane
understands: `reasoning_effort` for OpenAI-style providers, `thinkingLevel` for Gemini 3 on
the standard lane, and `thinkingBudget` on the native Vertex lane (512, 1024, 4096, 8192,
16384 or 24576 tokens for `minimal` through `max`). [Effort](../configuration/routes.md#effort)
has the full mapping.

`none` sends nothing, so the model's default applies. On Gemini 2.5 Pro and Flash that
default is dynamic thinking, which can cost more than `minimal`.

A client's own effort always wins on its lane:

- On the standard lane, a `reasoning_effort` in the request body replaces the tier's effort.
- On the native Vertex lane, a `vertex.thinking_config` replaces the tier's `thinkingBudget`.
  The native lane ignores `reasoning_effort`, so sending only `reasoning_effort` on a native
  request leaves the tier's budget in place.

When the client's effort applies, Jev still picks the tier and the response reports
`x-synapse-reasoning-effort: client`.

## Tier fallback

The chosen tier's legs run first, in order. If they all fail, Synapse tries each harder
tier, nearest first, then each easier tier, nearest first. For a four-tier route where Jev
picked the second tier, the order is tiers 2, 3, 4, 1. Each leg keeps its own tier's effort.

Requests with [native Vertex features](native-vertex.md) can only use `vertex` legs. If the
chosen tier has none, the nearest tier that does serves the request, harder tiers first. If
no tier has a `vertex` leg, the request fails with `400`.

What counts as a leg failure depends on the lane; see [Fallback chains](fallback-chains.md).

## Override the decision per request

- `"routing_strategy": "static"` skips Jev for one request. It is served from
  `default_tier` with each tier's configured effort, never raised, and falls back in the same
  order. Use it for latency-sensitive calls or to compare against a fixed tier.
- `"routing_strategy": "jev"` is accepted on a `jev` route and changes nothing. On a static
  route it returns `400`, as does any other value.
- A `jev` block, with `questions` or `extract`, returns `400` on a `jev` route; the
  [Jev lane](jev-lane.md) needs a route with `typesafe` legs.

## Response headers

Every chat completion response carries `x-synapse-routing`; the other headers appear when
they apply:

| Header | Value |
|---|---|
| `x-synapse-routing` | `jev` when Jev decided, `static-override` when the client sent `"routing_strategy": "static"`, `static` on routes without tiers. |
| `x-synapse-tier` | The tier whose leg served the request. |
| `x-synapse-tier-decided` | The tier Jev chose, only when a different tier served. |
| `x-synapse-reasoning-effort` | The effort the serving leg ran with, or `client`. |
| `x-synapse-routing-degraded` | Why Jev's decision was not used: `timeout`, `error`, `low_confidence` or `jev_unavailable`. |

Streaming responses report the leg that produced the first chunk, since the headers are
sent before the body.

## Failure behaviour

A Jev problem never fails the request. When Jev cannot decide, the request goes to
`default_tier`, with its configured effort, and the response carries
`x-synapse-routing-degraded`:

| Reason | Cause |
|---|---|
| `timeout` | Jev did not answer within `timeout_ms`. |
| `error` | A connection error, a non-success response, or an answer Synapse could not parse. |
| `low_confidence` | Jev answered with confidence below `min_confidence`. The effort can still be raised. |
| `jev_unavailable` | The gateway has no Jev client, which happens when an embedding application builds the gateway without one. |

Failed and timed-out decisions are logged as warnings with target `synapse::routing`,
carrying only the failure kind (`timeout`, `transport`, `http_status`, `unreadable_body` or
`unparseable_answers`) and the HTTP status or configured timeout, never the response body,
which may echo customer text.

If the serving tier's legs fail, [tier fallback](#tier-fallback) takes over. When every leg
of every eligible tier fails, the request fails as on a static route: `502` with
`all_legs_failed` on the standard lane, or `502` with `upstream_error` on the native Vertex
lane.

### Without a TypeSafe key

A `jev` route references the `typesafe` provider, so under the default strict provider
validation the gateway refuses to start without `TYPESAFE_API_KEY`. Under lenient
validation it starts and downgrades the route to a static route whose legs run in the order
`default_tier`, each harder tier, each easier tier. The legs keep their tier's effort, so
responses carry `x-synapse-routing: static` and `x-synapse-reasoning-effort`, but no
`x-synapse-tier`. See [Providers](../configuration/providers.md#strict-and-lenient-validation).

## Ledger rows

Each decision Jev answers writes its own ledger row, with provider `typesafe`, the
`jev_router.model` as model, lane `jev` and the same `request_id` as the chat row it routed.
It is priced as `typesafe:<model>` in `pricing.toml`. Timed-out, failed and overridden
decisions write no row. In events published to Pub/Sub or SNS, decision rows have
`op = "route_decision"`; the SQLite and Postgres tables have no `op` column, so identify
decisions there by provider and lane. See [Cost ledger](cost-ledger.md).

## Metrics and logs

| Metric | Labels | Description |
|---|---|---|
| `synapse_routing_decisions_total` | `route`, `tier`, `outcome` | One per planned request on a `jev` route. `tier` is the decided tier (`default_tier` when degraded); `outcome` is `decided`, `low_confidence`, `timeout`, `error` (including `jev_unavailable`) or `static_override`. |
| `synapse_routing_decision_duration_seconds` | `route` | Latency of each Jev call made, including calls that time out. |

Each planned request also emits an `info` event with target `synapse::routing` and the
message `route planned`, carrying the mode, decided tier, outcome, effort, and Jev's
difficulty score, confidence and reasoning probability. Requests rejected with `400` or by
guardrails emit neither the metric nor the event.

A rising share of `timeout` or `error` outcomes means requests are landing on
`default_tier` without a real decision; raise `timeout_ms` or check Jev's availability.

## In an embedded gateway

Applications that embed the gateway read the same information from `Gateway::chat_routed`,
which returns a `RoutingReport` next to the response, or from `GuardedStream::routing()` for
streams. See [Embedding Synapse as a library](embedding-as-library.md#read-the-routing-report).
