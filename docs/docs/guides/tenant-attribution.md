---
sidebar_position: 7
title: Tenant attribution
description: Attribute every request to a tenant, workspace, user, thread and task type with x-synapse-* headers, and classify the work with AI task types.
---

Use tenant attribution when more than one team, customer or application shares a gateway
and you need to know who spent what. Clients add `x-synapse-*` headers to their requests,
and Synapse records them on every [cost ledger](cost-ledger.md) row, so you can report usage
and cost per tenant, per workspace, per end user or per kind of work without changing the
request body.

## Attribution headers

| Header | Recorded as | Description |
|---|---|---|
| `x-synapse-tenant` | `tenant` | The customer, team or application. Without the header, `SYNAPSE_DEFAULT_TENANT` (default `unattributed`). |
| `x-synapse-workspace` | `workspace` | A grouping within the tenant, such as a project or team. |
| `x-synapse-user` | `user_id` | The end user within the tenant. |
| `x-synapse-thread` | `thread_id` | A conversation or agent thread. |
| `x-synapse-message` | `message_id` | A message or unit of work within the thread. Also becomes the request id; see [Correlate requests](#correlate-requests). |
| `x-synapse-user-task-type` | `user_task_type` | Your own label for the work the request serves, such as `summarisation`. Recorded as sent and never interpreted. |
| `x-synapse-ai-task-type` | `ai_task_type` | Overrides the [AI task type](#ai-task-types) for this request. |

All headers are optional. They work the same on `POST /v1/chat/completions`,
`POST /v1/embeddings` and the passthrough endpoints. For example:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -H "x-synapse-workspace: onboarding" \
  -H "x-synapse-user: user-42" \
  -H "x-synapse-user-task-type: summarisation" \
  -d '{
    "model": "chat",
    "messages": [{"role": "user", "content": "Summarise our onboarding guide."}]
  }'
```

Synapse records the values as sent. A header that is present but empty records an empty
string rather than falling back to the default.

:::warning
Synapse does not authenticate these headers: any client can claim any tenant. If
attribution drives billing or quotas, run Synapse behind a gateway or service mesh that
authenticates callers and sets `x-synapse-tenant` itself, overwriting any value the client
sent.
:::

## Where attribution goes

Every ledger row has a column for each of the seven fields. `tenant` and `ai_task_type` are
always set; the others are null when the header was not sent. Events published to Pub/Sub or
SNS carry the same fields in camelCase, omitting absent ones, with the tenant as
`namespace`. See [Cost ledger](cost-ledger.md) for the full
row and event format.

None of them are metric labels. Their values come from clients and are unbounded, and
putting them on metrics would create a new series for every tenant, user or thread. Query
the ledger for per-tenant numbers, and use the metrics for per-route, per-model and per-lane
views.

Set `SYNAPSE_DEFAULT_TENANT` to something recognisable, such as the name of the deployment,
so unattributed usage stands out. See [Environment variables](../configuration/environment-variables.md#tenancy).

## Correlate requests

Each request gets a request id, recorded as the ledger's `request_id`. For chat
completions it is also the response `id`, as `chatcmpl-<request id>`. When the request has an `x-synapse-message`
header, that value is the request id; otherwise Synapse generates a UUID.

Stamping `x-synapse-message` lets you join a ledger row to the message in your own system.
All rows a single request writes share its request id: on a `strategy = "jev"` route, the
routing decision and the chat completion; with hybrid extraction, the Jev answer and every
extraction. Reusing one message id for several requests gives their rows the same
`request_id`.

## AI task types

The AI task type classifies the work the gateway performed, such as `conversation`,
`extraction` or `vision`, so you can report cost by kind of work across routes and tenants.
Every ledger row has one. Synapse resolves it per request:

1. the `x-synapse-ai-task-type` header, when present and non-empty;
2. otherwise the task type mapped to the request's route alias in `ai_task_types.toml`;
3. otherwise `simple`.

`ai_task_types.toml` is keyed by task type, each listing the route aliases that perform
that kind of work:

```toml
conversation = ["chat", "planning"]
extraction = ["extract", "doc-extract"]
vision = ["pdf-ocr"]
```

- The gateway reads it from `SYNAPSE_AI_TASK_TYPES_PATH` (default
  `config/ai_task_types.toml`) at startup. The file is optional: without it, every request
  resolves to `simple` unless the header says otherwise.
- Aliases not listed resolve to `simple`, so list only the routes whose work is not simple.
- An alias listed under two task types stops the gateway at startup; listing it twice under
  the same type is allowed.
- For embeddings, the alias is the embedding alias.

The two task type fields answer different questions. `user_task_type` is your application's
own label, recorded only when a client sends it. `ai_task_type` is always set, and is
controlled centrally in `ai_task_types.toml`, so reports stay consistent even when clients
don't send anything.

## In an embedded gateway

Applications that embed the gateway pass the same fields in a `RequestCtx`, whose `tenant`,
`workspace`, `user`, `thread`, `message`, `user_task_type` and `ai_task_type` fields match the
headers, plus an optional `request_id`. See [Embedding Synapse as a library](embedding-as-library.md).
