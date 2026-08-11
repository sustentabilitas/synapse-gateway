# synapse-a2a

In-memory A2A agent registry for **synapse-gateway**: admin registration plus public catalog / agent-card / resolve endpoints.

Mounted on the LLM gateway HTTP listener (default `:8080`) — a stable cluster service that marketplace and ploutonion register against. This is **not** the cortex-sandbox-broker MCP plane.

## How it fits

```
ploutonion ──POST /internal/a2a/agents──▶ synapse-gateway :8080
marketplace ──GET  /.well-known/a2a-agent-catalog.json──▶ same
clients     ──GET  /a2a/agents/{id}/…──▶ same
```

One `Arc<A2aRegistry>` is created at process boot in `synapse-gateway` and shared by the admin and public routers via `.merge(...)`.

## Static seed

Agents may be seeded at process boot via `config/a2a.toml` (`SYNAPSE_A2A_PATH`):

```toml
[[a2a_agents]]
id = "ghg-emissions"
name = "GHG Emissions"
description = "Estimates GHG emissions"
endpoint_url = "http://ploutonion/a2a/agents/ghg-emissions"
card_url = "http://ploutonion/a2a/agents/ghg-emissions/.well-known/agent-card.json"
tags = ["ghg", "emissions"]
```

At boot the gateway GETs each `card_url` (3 attempts, exponential backoff) and insert-only registers the agent. Missing seed file ⇒ empty registry. Unreachable / invalid card after retries ⇒ process fails to start.

## Registration semantics

`POST /internal/a2a/agents` and seed inserts are **first-writer-wins**: re-adding an existing `id` is ignored and still returns `204`. Use `DELETE` then `POST` to replace.

## Admin (register if absent / deregister)

No auth (same pattern as gateway-internal surfaces).

### `POST /internal/a2a/agents`

Register if absent (ignore duplicate). Body matches `RegisterA2aAgentRequest`.

**Request**

```http
POST /internal/a2a/agents
Content-Type: application/json
```

```json
{
  "id": "ghg-emissions",
  "name": "GHG Emissions",
  "description": "Estimates GHG emissions",
  "endpoint_url": "http://ploutonion/a2a/agents/ghg-emissions",
  "card_url": "http://ploutonion/a2a/agents/ghg-emissions/.well-known/agent-card.json",
  "tags": ["ghg", "emissions"],
  "card": {
    "name": "GHG Emissions",
    "description": "Estimates GHG emissions",
    "url": "http://ploutonion/a2a/agents/ghg-emissions",
    "version": "1.0",
    "skills": []
  },
  "ttl_seconds": 3600
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `id` | string | Registry key |
| `name` | string | Display name |
| `description` | string | Short summary |
| `endpoint_url` | string | Absolute A2A JSON-RPC URL |
| `card_url` | string | Absolute agent-card URL |
| `tags` | string[] | Free-form tags |
| `card` | object | Full A2A agent-card JSON (passthrough) |
| `ttl_seconds` | u64? | Optional TTL; omit for no expiry |

**Response:** `204 No Content`

### `DELETE /internal/a2a/agents/{id}`

Deregister by id.

**Response:** `204 No Content` (idempotent even if missing)

## Public discovery

### `GET /.well-known/a2a-agent-catalog.json`

Lists non-expired agents. Response matches `A2aCatalog`.

```json
{
  "version": "1.0",
  "agents": [
    {
      "id": "ghg-emissions",
      "name": "GHG Emissions",
      "description": "Estimates GHG emissions",
      "card_url": "http://ploutonion/a2a/agents/ghg-emissions/.well-known/agent-card.json",
      "endpoint_url": "http://ploutonion/a2a/agents/ghg-emissions",
      "tags": ["ghg", "emissions"]
    }
  ]
}
```

### `GET /a2a/agents/{id}/.well-known/agent-card.json`

Returns the stored `card` JSON for `{id}`.

**Response:** `200` + card object, or `404` if unknown / expired.

```json
{
  "name": "GHG Emissions",
  "description": "Estimates GHG emissions",
  "url": "http://ploutonion/a2a/agents/ghg-emissions",
  "version": "1.0",
  "skills": []
}
```

### `GET /a2a/agents/{id}/resolve`

Returns endpoint + card. Response matches `A2aResolveResponse`.

**Response:** `200`, or `404` if unknown / expired.

```json
{
  "id": "ghg-emissions",
  "endpoint_url": "http://ploutonion/a2a/agents/ghg-emissions",
  "card_url": "http://ploutonion/a2a/agents/ghg-emissions/.well-known/agent-card.json",
  "card": {
    "name": "GHG Emissions",
    "description": "Estimates GHG emissions",
    "url": "http://ploutonion/a2a/agents/ghg-emissions",
    "version": "1.0",
    "skills": []
  }
}
```

## TTL

When `ttl_seconds` is set at register time, `resolve` / catalog listing drop the agent after expiry (same seam as `McpRegistry`: `resolve_at` / `list_at`).

## Crate API

```rust
use std::sync::Arc;
use synapse_a2a::{a2a_admin_router, a2a_public_router, A2aRegistry};

let registry = Arc::new(A2aRegistry::new());
let app = axum::Router::new()
    .merge(a2a_admin_router(registry.clone()))
    .merge(a2a_public_router(registry));
```
