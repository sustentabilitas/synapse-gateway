---
sidebar_position: 3
title: Fall back across providers
description: Break the first leg of a two-provider route on purpose, watch OpenAI serve the request instead, and learn how fallback differs for streaming and non-streaming clients.
---

In this tutorial you make the first leg of the quickstart's `chat` route fail on purpose and
watch its second leg, OpenAI, answer instead. Along the way you see what a client receives
when every leg fails, and how fallback works for streaming and non-streaming requests.

## Before you begin

Complete the [Quickstart](../quickstart.md), including an `OPENAI_API_KEY`: this tutorial
needs both legs of the `chat` route. It runs from the same `synapse-quickstart` directory.

## Check the healthy route

With the quickstart configuration, send a request to `chat`:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "chat",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

The response's `model` field names the leg that answered: `gemini-3.5-flash-lite`, the
route's first leg.

## Break the first leg

Replace `config/routes.toml` with this file, which points the first leg of `chat` at a
model that does not exist:

```toml title="config/routes.toml"
[routes."gemini-flash"]
legs = [
  { provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" },
]

[routes."chat"]
legs = [
  { provider = "vertex", model = "gemini-does-not-exist", region = "us" },
  { provider = "openai", model = "gpt-4o-mini" },
]
```

Synapse reads its configuration at startup, so stop the gateway and start it again with the
same command as in the quickstart.

## Watch the second leg serve

Send the same request again:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "chat",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

This time the response comes back with `"model": "gpt-4o-mini"`. Vertex AI rejected the
first leg, so Synapse moved on to the next leg in the route and returned OpenAI's answer.
The client sent the same request and got one successful response; it never saw the failed
attempt.

The ledger records the leg that served the request:

```bash
sqlite3 data/synapse.db \
  "SELECT route, provider, model, cost_usd FROM usage_events ORDER BY id DESC LIMIT 1;"
```

The latest row shows route `chat`, provider `openai` and model `gpt-4o-mini`, priced with
the `openai:gpt-4o-mini` entry in `pricing.toml`.

## Stream with fallback

Streaming requests fall back too, as long as nothing has reached the client yet:

```bash
curl -sN http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "chat",
    "messages": [{"role": "user", "content": "Count to 5."}],
    "stream": true
  }'
```

The first leg fails before producing any output, so Synapse starts the stream from the
OpenAI leg, and every `chat.completion.chunk` carries `"model": "gpt-4o-mini"`.

## When every leg fails

Restart the gateway with `OPENAI_API_KEY=sk-invalid` so that the second leg fails too, and
send the non-streaming request again. Synapse returns `502` with error code
`all_legs_failed` and one entry per failed leg:

```json
{
  "error": {
    "type": "all_legs_failed",
    "code": "all_legs_failed",
    "message": "all legs of route 'chat' failed",
    "failures": [
      { "provider": "vertex", "model": "gemini-does-not-exist", "message": "..." },
      { "provider": "openai", "model": "gpt-4o-mini", "message": "..." }
    ]
  }
}
```

Each `message` carries the provider's error, which tells you why the leg failed.

## Streaming and non-streaming fallback

Synapse always streams from the provider internally, so the difference between the two
kinds of client is what has already been sent when a leg fails:

- **Non-streaming clients** get the whole chain. Synapse buffers each leg's stream into one
  response before sending anything, so any failure moves to the next leg: an error
  response, no first chunk within `SYNAPSE_REQUEST_TIMEOUT_SECS` (default 120), a stall
  longer than `SYNAPSE_STREAM_IDLE_TIMEOUT_SECS` (default 60) or a stream that breaks
  halfway.
- **Streaming clients** get fallback until the first chunk. Synapse commits to the first
  leg that produces a chunk and forwards it straight away. If that leg fails later, it is
  too late to switch: the stream ends with a
  `data: {"error": {"type": "upstream_error", ...}}` event followed by `data: [DONE]`.

These rules apply to the standard lane, which served every request in this tutorial. A
native Vertex request uses only the route's `vertex` legs, so on `chat` the OpenAI leg
cannot rescue it. Between `vertex` legs, the native lane moves on only after a `5xx`, `429`
or `408` response, a connection error or a timeout; see
[Request flow](../../overview/architecture.md#request-flow).

## Restore the route

Set the first leg of `chat` back to `gemini-3.5-flash-lite`, restore your real
`OPENAI_API_KEY` and restart the gateway.

## Next steps

- Look up the [fallback rules](../../configuration/routes.md#fallback) in the routes
  reference.
- Read [Architecture](../../overview/architecture.md) for how fallback fits in the request
  flow.
- Look up [fallback chains](../../overview/concepts.md#fallback-chain) in Concepts.
