---
sidebar_position: 2
title: Providers
description: The provider ids a route leg can use, the environment variables each one needs, and how strict and lenient validation treat missing credentials.
---

Every leg in `routes.toml` names a `provider`. Synapse knows five provider ids, and each
reads its credentials and endpoint from environment variables. At startup, the gateway
checks that every provider your routes reference has what it needs.

## Provider ids

| Provider id | Calls | Required | Optional |
|---|---|---|---|
| `vertex` | Google Vertex AI (Gemini) | `VERTEX_PROJECT_ID` (or legacy `VERTEX_PROJECT`), plus Application Default Credentials | `VERTEX_LOCATION` (default `global`) |
| `openai` | OpenAI | `OPENAI_API_KEY` | `OPENAI_BASE_URL` (default `https://api.openai.com/v1`) |
| `qwen` | Alibaba Cloud DashScope (Qwen) | `DASHSCOPE_API_KEY` | `DASHSCOPE_BASE_URL` (default `https://dashscope-intl.aliyuncs.com/compatible-mode/v1`) |
| `oai_compat` | Any OpenAI-compatible server, such as vLLM, Ollama or TGI | `OAI_COMPAT_BASE_URL` | `OAI_COMPAT_API_KEY` |
| `typesafe` | TypeSafe System One (Jev) | `TYPESAFE_API_KEY` | `TYPESAFE_BASE_URL` (default `https://api.typesafe.ai`) |

## `vertex`

Synapse authenticates to Vertex AI with Application Default Credentials: a service-account
key named by `GOOGLE_APPLICATION_CREDENTIALS`, your `gcloud auth application-default login`
credentials, or the metadata server on Google Cloud. At startup it only checks that the
project is set. Missing or invalid credentials surface on the first request.

Vertex legs work on two lanes:

- On the **standard lane**, Synapse calls Vertex AI's `global` endpoint through the `genai`
  crate. The leg's `region` is ignored.
- On the **native Vertex lane**, Synapse calls the Vertex REST API in the leg's `region`,
  or in `VERTEX_LOCATION` when the leg has none. `global` uses
  `aiplatform.googleapis.com`, the multi-regions `us` and `eu` use
  `aiplatform.us.rep.googleapis.com` and `aiplatform.eu.rep.googleapis.com`, and a single
  region such as `us-central1` uses `us-central1-aiplatform.googleapis.com`.

## `openai`, `qwen` and `oai_compat`

These providers speak the OpenAI chat completions API and run on the standard lane through
the `genai` crate. Point a base URL variable at a proxy or a regional endpoint to change
where requests go. `oai_compat` needs no API key; set `OAI_COMPAT_API_KEY` if your server
checks one.

## `typesafe`

`typesafe` legs run on the Jev lane: they answer requests that carry a `jev` block with
typed questions. A route with a `typesafe` leg returns `400` to requests without one.

A `strategy = "jev"` route also references `typesafe`, because Jev makes its routing
decision, even though its tiers cannot use `typesafe` legs. `TYPESAFE_API_KEY` also enables
the `POST /typesafe/v1/systemone` passthrough.

## Embedding aliases

Embedding aliases, declared under `[embeddings."<alias>"]` in `routes.toml`, support only
the `vertex` and `openai` providers. Validation covers their legs too. See the
[embeddings guide](../guides/embeddings.md).

## Strict and lenient validation

`SYNAPSE_PROVIDER_VALIDATION` decides what happens when a route references a provider this
process cannot build, because its required variable is unset or the provider id is
unknown:

- **`strict`** (the default) refuses to start and names the missing variable, for example
  `route references provider 'openai' but OPENAI_API_KEY is unset`. A dedicated gateway
  should fail at boot rather than serve a route table it cannot honour.
- **`lenient`** drops the legs it cannot serve, logs a warning for each provider, and
  starts. A route keeps its remaining legs; a route left with no legs disappears, so
  requests to it return `404` with error code `model_not_found`.

Lenient validation exists for route tables shared by several processes, where a leg added
for one consumer should not stop the others. It also prunes `strategy = "jev"` routes: legs
are removed inside each tier, empty tiers are dropped and `default_tier` is re-picked. If
`TYPESAFE_API_KEY` is missing or fewer than two tiers remain, the route downgrades to a
static route whose legs run in the order `default_tier`, each harder tier, then each easier
tier. See [Routes](routes.md#jev-routes).

Any value of `SYNAPSE_PROVIDER_VALIDATION` other than `lenient` means `strict`.
