---
sidebar_position: 2
title: Quickstart
description: Run the Synapse gateway with a two-route config and send standard, streaming and native Vertex AI requests to it.
---

In this quickstart you write a two-route configuration, start the gateway, and send it a
standard request, a streaming request and a native Vertex AI request.

## Prerequisites

- Docker, or Synapse installed with Cargo (see [Installation](installation.md)).
- A Google Cloud project with the Vertex AI API enabled, and credentials that can call it:
  a service-account key with the Vertex AI User role for Docker, or
  `gcloud auth application-default login` for a local binary.
- Optionally, an OpenAI API key for the second leg of the `chat` route. Without one, see the
  tip under [Start the gateway](#start-the-gateway).

The gateway starts without valid credentials: at startup it only checks that
`VERTEX_PROJECT_ID` and `OPENAI_API_KEY` are set. Credentials are used on the first request,
which fails with `502` and error code `all_legs_failed` if they are missing or invalid.

## Create the configuration

Create a working directory with a `config/` folder:

```bash
mkdir -p synapse-quickstart/config && cd synapse-quickstart
```

Save the routes as `config/routes.toml`:

```toml title="config/routes.toml"
[routes."gemini-flash"]
legs = [
  { provider = "vertex", model = "gemini-2.5-flash", region = "us-central1" },
]

[routes."chat"]
legs = [
  { provider = "vertex", model = "gemini-2.5-flash", region = "us-central1" },
  { provider = "openai", model = "gpt-4o-mini" },
]
```

Each `[routes."<alias>"]` table defines a model name your clients can send. `gemini-flash`
always uses Gemini 2.5 Flash on Vertex AI. `chat` tries Gemini first and falls back to
OpenAI's `gpt-4o-mini` if the Vertex leg fails. `region` pins the Vertex leg to
`us-central1` on the native Vertex lane.

Save the prices as `config/pricing.toml`:

```toml title="config/pricing.toml"
"vertex:gemini-2.5-flash" = { input = 0.30, output = 2.50 }
"openai:gpt-4o-mini" = { input = 0.15, output = 0.60 }
```

Prices are in USD per 1,000,000 tokens, keyed by `provider:model`. Synapse uses them to
price every request in the cost ledger; a model without a price costs 0.

## Start the gateway

### Docker

Put your service-account key in the working directory as `sa.json`, then run:

```bash
docker run --rm -p 8080:8080 -p 9090:9090 \
  -e VERTEX_PROJECT_ID=my-gcp-project \
  -e OPENAI_API_KEY=sk-... \
  -e GOOGLE_APPLICATION_CREDENTIALS=/secrets/sa.json \
  -v "$(pwd)/sa.json:/secrets/sa.json:ro" \
  -v "$(pwd)/config:/app/config" \
  sustentabilitas/synapse-gateway
```

The image runs as the non-root user `synapse` (UID 1001) with no home directory, so it
cannot see your `gcloud` credentials; the mounted key takes their place. On Linux, make sure
UID 1001 can read `sa.json`.

Mounting `config/` replaces the image's whole `/app/config` directory, so only the files you
provide are used. Guardrails stay off until you add a `guardrails.toml`.

### Cargo

From the `synapse-quickstart` directory, with the binary from `cargo install synapse-gateway`:

```bash
gcloud auth application-default login
export VERTEX_PROJECT_ID=my-gcp-project
export OPENAI_API_KEY=sk-...
SYNAPSE_ROUTES_PATH=config/routes.toml SYNAPSE_PRICING_PATH=config/pricing.toml \
  synapse-gateway
```

To run from a clone of the repository instead, replace `synapse-gateway` with
`cargo run --release -p synapse-gateway --manifest-path <path-to-clone>/Cargo.toml`. The
paths stay relative to your working directory.

Either way, the API listens on port `8080` and Prometheus metrics on port `9090`.

:::tip
No OpenAI key? Set `SYNAPSE_PROVIDER_VALIDATION=lenient` instead of `OPENAI_API_KEY`.
Synapse then drops the `openai` leg and starts with `chat` served by Vertex alone. The
default, `strict`, refuses to start when a route references a provider without credentials.
:::

## Check that it is running

```bash
curl -s http://localhost:8080/health
```

The response is the plain-text body `ok`.

List the model aliases your clients can use:

```bash
curl -s http://localhost:8080/v1/models
```

```json
{"object":"list","data":[{"id":"chat","object":"model","owned_by":"synapse"},{"id":"gemini-flash","object":"model","owned_by":"synapse"}]}
```

## Send a standard request

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "gemini-flash",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

The response is a standard OpenAI `chat.completion` object. The `x-synapse-tenant` header
attributes the request's tokens and cost to `my-team` in the ledger; without it, requests are
attributed to `unattributed`.

Because the request is plain OpenAI, any OpenAI SDK works too: point its base URL at
`http://localhost:8080/v1` and use `gemini-flash` as the model.

## Send a streaming request

```bash
curl -sN http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "gemini-flash",
    "messages": [{"role": "user", "content": "Count to 5."}],
    "stream": true
  }'
```

The response is a stream of server-sent events in the OpenAI format: `data: {...}` lines
carrying `chat.completion.chunk` objects, ending with `data: [DONE]`.

## Send a native Vertex request

Add a `vertex` block to use Vertex-only features. This request attaches a video from Cloud
Storage and reuses a context cache:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "gemini-flash",
    "messages": [{"role": "user", "content": "Describe this video."}],
    "vertex": {
      "media_uris": ["gs://my-bucket/video.mp4"],
      "cached_content": "projects/my-gcp-project/locations/us-central1/cachedContents/abc123"
    }
  }'
```

Replace the values with your own: Vertex AI in your project must be able to read the Cloud
Storage object, and the cache must exist for `gemini-2.5-flash` in `us-central1`, the model
and region of the route's Vertex leg. Remove `cached_content` if you do not have a cache.

The `vertex` block sends the request to the native Vertex lane, which calls Vertex AI's
`generateContent` API directly and keeps these fields. Only `vertex` legs serve it: on the
`chat` route, the OpenAI leg is skipped. A route with no `vertex` leg returns `400` with
error code `native_feature_unsupported`.

## See what it cost

With the default SQLite ledger, the gateway writes one row per completed request to
`synapse.db` in its working directory. If you ran the binary locally:

```bash
sqlite3 synapse.db \
  'SELECT tenant, route, provider, model, lane, input_tokens, output_tokens, cost_usd FROM usage_events;'
```

In the Docker container the file is `/app/synapse.db` and is discarded with the container.

## Next steps

- Read [Architecture](../overview/architecture.md) to see how lanes and fallback chains fit
  together.
- Look up routes, legs, tiers and tenants in [Concepts](../overview/concepts.md).
