---
sidebar_position: 1
title: Cache a video and get structured answers
description: Create a Vertex AI context cache, query it through Synapse's native Vertex lane with a response schema, and see the requests in the cost ledger and metrics.
---

In this tutorial you cache a video in Vertex AI once, ask Gemini questions about it through
Synapse without sending the video again, and constrain the answers to a JSON schema. Both
features only exist on Vertex AI, so Synapse serves these requests on its native Vertex
lane. At the end you look up the requests in the cost ledger and the metrics.

## Before you begin

- Complete the [Quickstart](../quickstart.md) and keep the gateway running with its
  `gemini-flash` route. This tutorial runs from the same `synapse-quickstart` directory.
- Install the [Google Cloud CLI](https://cloud.google.com/sdk/docs/install) and sign in
  with an account that can use Vertex AI in your project (`gcloud auth login`).

The examples use the project `my-gcp-project`; replace it with yours.

## Create a context cache

A context cache stores content, here a 98-second public sample video, in Vertex AI so
later requests can refer to it by name instead of sending it again. Vertex AI bills cached
tokens at a discount and charges for storage until the cache expires. See Google's guide to
[creating a context cache](https://cloud.google.com/vertex-ai/generative-ai/docs/context-cache/context-cache-create)
for all the options.

The cache must be for the same model and location as the route's Vertex leg. The
`gemini-flash` route uses `gemini-3.5-flash-lite` in the `us` multi-region, so create the
cache there:

```bash
curl -s -X POST \
  -H "Authorization: Bearer $(gcloud auth print-access-token)" \
  -H "Content-Type: application/json" \
  https://aiplatform.us.rep.googleapis.com/v1/projects/my-gcp-project/locations/us/cachedContents \
  -d '{
    "model": "projects/my-gcp-project/locations/us/publishers/google/models/gemini-3.5-flash-lite",
    "displayName": "synapse-tutorial-animals",
    "contents": [{
      "role": "user",
      "parts": [{
        "fileData": {
          "mimeType": "video/mp4",
          "fileUri": "gs://cloud-samples-data/video/animals.mp4"
        }
      }]
    }],
    "ttl": "3600s"
  }'
```

The response describes the new cache:

```json
{
  "name": "projects/123456789012/locations/us/cachedContents/1234567890123456789",
  "model": "projects/my-gcp-project/locations/us/publishers/google/models/gemini-3.5-flash-lite",
  "createTime": "2026-09-28T10:00:00.000000Z",
  "updateTime": "2026-09-28T10:00:00.000000Z",
  "expireTime": "2026-09-28T11:00:00.000000Z"
}
```

Copy the `name` value into a shell variable. The cache expires after the one-hour `ttl`:

```bash
export CACHE="projects/123456789012/locations/us/cachedContents/1234567890123456789"
```

:::note
Vertex AI only caches content above a minimum size: 4,096 tokens for Gemini 3 models. The
sample video is well above it. If you cache your own content and the create call fails,
check the limits in Google's
[context caching overview](https://cloud.google.com/vertex-ai/generative-ai/docs/context-cache/context-cache-overview).
:::

## Ask a question about the cached video

Send a chat completion to `gemini-flash` with the cache name in the `vertex` block. The
video is not in the request; Gemini reads it from the cache:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d @- <<EOF
{
  "model": "gemini-flash",
  "messages": [{"role": "user", "content": "Which animals appear in the video?"}],
  "vertex": { "cached_content": "$CACHE" }
}
EOF
```

The response is a standard `chat.completion` whose message lists the animals. Because the
request has a `vertex` block with `cached_content`, Synapse sends it to the native Vertex
lane, which passes the name to Vertex AI as `cachedContent`. The standard lane would have
to drop that field.

If the cache does not exist, has expired, or was created for another model or location,
Vertex AI rejects the request and Synapse returns `400` with Vertex's error message.

## Get a structured answer

Add a `response_schema` to the `vertex` block to make Gemini answer with JSON that matches
the schema:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d @- <<EOF
{
  "model": "gemini-flash",
  "messages": [{"role": "user", "content": "List the animals in the video and describe the setting."}],
  "vertex": {
    "cached_content": "$CACHE",
    "response_schema": {
      "type": "object",
      "properties": {
        "animals": { "type": "array", "items": { "type": "string" } },
        "setting": { "type": "string" }
      },
      "required": ["animals", "setting"]
    }
  }
}
EOF
```

Synapse sends the schema as `generationConfig.responseSchema` with `responseMimeType` set to
`application/json`, so Vertex AI constrains decoding to the schema. The message content is
a JSON string you can parse directly:

```json
{"animals": ["..."], "setting": "..."}
```

Synapse leaves Gemini's thinking parts out of the content, so the string is only the JSON
document.

## See the requests in the ledger

Query the SQLite ledger for the two native-lane requests you just sent:

```bash
sqlite3 data/synapse.db \
  "SELECT route, lane, model, input_tokens, output_tokens, cost_usd
   FROM usage_events WHERE lane = 'native' ORDER BY id DESC LIMIT 2;"
```

Both rows show route `gemini-flash`, lane `native` and model `gemini-3.5-flash-lite`,
attributed to `my-team`. `input_tokens` is Vertex AI's `promptTokenCount`, which includes
the cached video tokens, so it is much larger than your short prompt.

:::warning
Synapse prices every input token at the `input` price in `pricing.toml`, cached or not.
Vertex AI bills cached tokens at a discount (90% on Gemini 2.5 and later) and charges
separately for cache storage, so the ledger overstates the cost of cached requests.
:::

## See the requests in the metrics

The metrics endpoint on port `9090` counts requests per route, model, provider system and
lane:

```bash
curl -s http://localhost:9090/metrics | grep 'lane="native"'
```

Look for `synapse_requests_total` and `synapse_input_tokens_total` series labelled
`route="gemini-flash"`, `lane="native"` and `system="vertexai"`.

## Clean up

Delete the cache to stop paying for its storage before it expires:

```bash
curl -s -X DELETE \
  -H "Authorization: Bearer $(gcloud auth print-access-token)" \
  "https://aiplatform.us.rep.googleapis.com/v1/$CACHE"
```

## Next steps

- [Route requests by difficulty with Jev](jev-tiers.md) to pick a model and reasoning
  effort per request.
- Read about the [native Vertex lane](../../overview/architecture.md#native-vertex-lane),
  including `media_uris` and `thinking_config`.
