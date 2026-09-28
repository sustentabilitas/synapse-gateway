---
sidebar_position: 4
title: Streaming and tool calling
description: How Synapse streams responses, how streaming interacts with fallback and timeouts, and how tool calling works on the standard and native Vertex lanes.
---

Use streaming when a person is waiting on the output, so text appears as the model writes
it; use non-streaming requests for background work, where the full fallback chain matters
more than the first token. Use tool calling when the model needs to call your functions.
Both work through the standard OpenAI request format, on the standard and native Vertex
lanes, so existing OpenAI SDK code works unchanged.

## Streaming

Set `"stream": true` to receive server-sent events: a sequence of `chat.completion.chunk`
objects, each on a `data: ` line, ending with `data: [DONE]`. The last chunk before
`[DONE]` has an empty `delta` and the `finish_reason`. Without `stream`, you get one
`chat.completion` object.

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

Streamed chunks carry no `usage` object. Synapse still counts the tokens: they are in the
[cost ledger](cost-ledger.md) and the `synapse_input_tokens_total` and
`synapse_output_tokens_total` metrics.

Synapse always streams from the provider, whatever the client asked for. For a
non-streaming client, it buffers the stream into one response before sending anything,
which is why non-streaming requests keep the whole fallback chain.

### Fallback while streaming

A streaming request can fall back only until its first chunk reaches the client. Synapse
commits to the first leg that produces a chunk; if that leg fails later, it is too late to
switch, because the client already has part of the answer. The HTTP status is already
`200`, so the failure arrives as an event:

```text
data: {"error":{"type":"upstream_error","code":"upstream_error","message":"..."}}

data: [DONE]
```

Clients should treat an `error` event as a failed response and discard the partial text.
Non-streaming requests on the standard lane never see this: a leg that breaks mid-way is
abandoned and the next leg starts from scratch. [Fallback chains](fallback-chains.md)
describes what advances the chain on each lane.

## Timeouts

Two settings bound how long Synapse waits for a provider, and they apply differently by
lane and by kind of client:

| Request | First chunk | Gap between chunks | Whole response |
|---|---|---|---|
| Standard lane, non-streaming | `SYNAPSE_REQUEST_TIMEOUT_SECS`; the next leg is tried | `SYNAPSE_STREAM_IDLE_TIMEOUT_SECS`; the next leg is tried | `SYNAPSE_REQUEST_TIMEOUT_SECS` |
| Standard lane, streaming | `SYNAPSE_REQUEST_TIMEOUT_SECS`; the next leg is tried | Not enforced | `SYNAPSE_REQUEST_TIMEOUT_SECS` |
| Native Vertex lane | Not enforced | Not enforced | `SYNAPSE_REQUEST_TIMEOUT_SECS` |

`SYNAPSE_REQUEST_TIMEOUT_SECS` (default 120) is also the HTTP timeout of every provider
call, and it covers the whole response, not only the wait for the first byte. A response
that takes longer than that to finish fails, even while tokens are still arriving: before
the first chunk it falls back like any other failure, after it the stream ends with an
`error` event. If you expect long generations, such as large structured outputs or heavy
thinking, raise it. See [Environment variables](../configuration/environment-variables.md#streaming-and-timeouts).

## Tool calling

Send OpenAI-style `tools` and Synapse translates them for the lane that serves the request.
When the model calls a tool, the assistant message has `content: null` and a `tool_calls`
array, and `finish_reason` is `tool_calls`:

```json
{
  "model": "chat",
  "messages": [{ "role": "user", "content": "What's the weather in Lisbon?" }],
  "tools": [{
    "type": "function",
    "function": {
      "name": "get_weather",
      "description": "Current weather for a city",
      "parameters": {
        "type": "object",
        "properties": { "city": { "type": "string" } },
        "required": ["city"]
      }
    }
  }]
}
```

Run the function, then send the conversation back with the assistant's tool call and a
`role: "tool"` message holding the result:

```json
{
  "model": "chat",
  "messages": [
    { "role": "user", "content": "What's the weather in Lisbon?" },
    { "role": "assistant", "content": null, "tool_calls": [{
      "id": "call_0", "type": "function",
      "function": { "name": "get_weather", "arguments": "{\"city\":\"Lisbon\"}" }
    }] },
    { "role": "tool", "tool_call_id": "call_0", "name": "get_weather", "content": "21°C, sunny" }
  ],
  "tools": [ ... ]
}
```

In streaming mode, tool calls arrive as `chat.completion.chunk` deltas with an `index` per
call. The first delta for a call carries its `id` and `function.name`; later deltas for the
same index carry only `function.arguments` fragments to concatenate.

The two lanes differ in a few details:

| | Standard lane | Native Vertex lane |
|---|---|---|
| Tool definitions | Translated for the provider by the `genai` crate. | Sent as Vertex `functionDeclarations`. |
| `tool_choice` | **Not forwarded.** The `genai` crate has no field for it, so the model decides. | Honoured through `toolConfig.functionCallingConfig`. See [Native Vertex features](native-vertex.md#tool-calling). |
| Tool call ids | The provider's ids. | Generated by Synapse: `call_0`, `call_1`, … |
| Streamed arguments | Fragments, as the provider sends them. | Each call's arguments in one delta. |
| Tool results | Matched by `tool_call_id`. | Sent to Vertex under the message's `name`, so set `name` to the function name. |

If you rely on `tool_choice` to force or forbid a tool call on a Gemini model, make the
request use the native Vertex lane by adding a `vertex` block with a native feature, such as
`thinking_config`. Otherwise, enforce the choice in your application.
