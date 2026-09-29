---
slug: jev-routing-explained
title: "Jev routing explained: the right model and effort for every request"
authors: [rajwilkhu]
tags: [routing]
description: How a strategy = "jev" route in Synapse asks TypeSafe Jev how demanding each request is, picks a tier and reasoning effort, reports the decision and degrades safely.
---

Most applications send every request on a route to the same model. A chat assistant that
answers "thanks!" and debugs a race condition in the same afternoon pays for its strongest
model on both, or saves money on both and disappoints on the hard one.

Synapse's Jev router lets the request decide. A `strategy = "jev"` route declares difficulty
tiers, and for each request the gateway asks TypeSafe Jev how demanding it is, then serves it
from the matching tier with a matching reasoning effort. This post walks through how that
decision is made, what it costs, and what happens when it can't be made.

<!-- truncate -->

## Tiers describe work, not models

A Jev route replaces a route's single list of legs with two to ten tiers, ordered from
easiest to hardest. Each tier has a name, a description of the work it suits, a reasoning
effort and its own legs:

```toml
[routes."auto"]
strategy = "jev"

[routes."auto".jev_router]
default_tier = "moderate"

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

The descriptions matter more than anything else in the file, because they are what Jev
scores the request against. "Multi-step analysis, maths or proofs" gives Jev something to
judge; "Gemini Pro" does not. The order matters too: it is what the difficulty score indexes,
and it drives fallback between tiers. The [Jev tiers tutorial](/docs/get-started/tutorials/jev-tiers/)
builds this route step by step.

## Two questions per request

Before serving a request, Synapse sends Jev a bounded summary of the conversation: the
latest user message, the system prompt, as much recent history as fits in a budget of about
24,000 characters, and whether the request carries tools or media. Jev answers two typed
questions about it.

**Difficulty** is a score against your tier descriptions, in order. Synapse rounds it to the
nearest tier. Jev also reports its confidence in that score; below `min_confidence`
(default 0.5), the gateway ignores the score and serves the request from `default_tier`.

**Needs reasoning** is the probability that a good reply needs careful step-by-step reasoning,
such as maths, logic, planning or debugging. At or above `reasoning_threshold` (default 0.7),
the tier's effort goes up one step, even when the difficulty score was too uncertain to use.

The decision call is bounded by `timeout_ms`, 400 ms by default, so it adds up to that much
latency to every request on the route. That is the price of the decision, and the reason a
static route is still the better choice when you already know which model each use case needs.

## Effort, translated per provider

Each tier's `effort` is one of `none`, `minimal`, `low`, `medium`, `high`, `xhigh` or `max`,
and the reasoning bump moves one step along that list, stopping at `max`. Synapse translates
the effort for each leg:

- On the standard lane, OpenAI-style providers get `reasoning_effort`, with `max` sent as
  `xhigh`, and Vertex Gemini 3 models get a `thinkingLevel`.
- On the native Vertex lane, Synapse sets `thinkingBudget` itself: 512 tokens for `minimal`,
  1,024 for `low`, 4,096 for `medium`, 8,192 for `high`, 16,384 for `xhigh` and 24,576 for
  `max`.
- `none` sends nothing, so the model's own default applies. On some Gemini models that default
  is dynamic thinking, so use `minimal` when you want thinking kept small.

The [Effort](/docs/configuration/routes/#effort) table in the routes reference has every
mapping. Thinking tokens are billed as output, which is why easy tiers usually run at `none`
or `minimal` and only hard tiers get `medium` or more.

A client's own effort always wins on its lane. A `reasoning_effort` in the request body
replaces the tier's effort on the standard lane, and a `vertex.thinking_config` replaces the
tier's `thinkingBudget` on the native lane. Jev still picks the tier, and the response
reports `x-synapse-reasoning-effort: client`.

## Every response says what happened

Every chat completion carries `x-synapse-routing`: `jev` on a Jev route, `static-override`
when the client sent `"routing_strategy": "static"` to skip the decision, and `static` on
ordinary routes. When they apply, `x-synapse-tier` names the tier that served,
`x-synapse-tier-decided` the tier Jev chose if a different one served, and
`x-synapse-reasoning-effort` the effort the serving leg ran with. For streams, the headers
describe the leg that produced the first chunk. The
[response headers](/docs/guides/jev-router/#response-headers) section lists every value.

## A Jev problem never fails the request

The router is designed to degrade, not to break. If Jev times out, errors, answers with low
confidence, or the gateway has no Jev client at all, the request goes to `default_tier` and
the response carries `x-synapse-routing-degraded` with the reason: `timeout`, `error`,
`low_confidence` or `jev_unavailable`. Pick a `default_tier` that handles most requests
acceptably, usually a middle one.

If the serving tier's legs fail, Synapse tries each harder tier, nearest first, then each
easier tier. Requests with native Vertex features only use `vertex` legs, so they go to the
nearest tier that has one. Only when every eligible leg fails does the request fail, exactly
as it would on a static route. [Failure behaviour](/docs/guides/jev-router/#failure-behaviour)
has the details.

## Decisions you can audit

Each decision Jev answers writes its own row to the [cost ledger](/docs/guides/cost-ledger/),
with provider `typesafe`, lane `jev` and the same `request_id` as the chat request it routed,
priced as `typesafe:<model>` from your `pricing.toml`. You see the cost of deciding next to
the cost of answering.

Two metrics track the router: `synapse_routing_decisions_total`, labelled by route, tier and
outcome, and `synapse_routing_decision_duration_seconds`. A rising share of `timeout` or
`error` outcomes means requests are landing on `default_tier` without a real decision, so
raise `timeout_ms` or check Jev's availability. [Metrics](/docs/operating/metrics/#queries)
has a ready-made query for that ratio.

## Try it

Start with the [Jev tiers tutorial](/docs/get-started/tutorials/jev-tiers/), then keep the
[Jev router guide](/docs/guides/jev-router/) at hand while you tune tier descriptions and
thresholds against your own traffic.
