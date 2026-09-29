---
sidebar_position: 4
title: Discovery
description: List registered A2A agents, fetch an agent's card, and resolve an agent id to its endpoint through the gateway's public A2A endpoints.
---

Clients use three public endpoints to find agents. They are read-only and served on the
gateway's API port. None of them contacts the agents: they return what was registered.

## `GET /.well-known/a2a-agent-catalog.json`

Lists every agent that hasn't expired:

```json
{
  "version": "1.0",
  "agents": [
    {
      "id": "invoice-agent",
      "name": "Invoice agent",
      "description": "Extracts line items from invoices",
      "card_url": "https://agents.example.com/invoice/.well-known/agent-card.json",
      "endpoint_url": "https://agents.example.com/invoice",
      "tags": ["finance"]
    }
  ]
}
```

`version` is always `"1.0"`. The catalogue doesn't include the cards, and the order of
`agents` is not defined, so sort it yourself if you display it.

## `GET /a2a/agents/{id}/.well-known/agent-card.json`

Returns the agent's card exactly as it was registered or fetched at startup:

```json
{
  "name": "Invoice agent",
  "description": "Extracts line items from invoices",
  "url": "https://agents.example.com/invoice",
  "version": "1.0",
  "skills": []
}
```

Returns `404` with an empty body if the id is unknown or has expired.

## `GET /a2a/agents/{id}/resolve`

Returns the endpoint to call, with the card, in one request:

```json
{
  "id": "invoice-agent",
  "endpoint_url": "https://agents.example.com/invoice",
  "card_url": "https://agents.example.com/invoice/.well-known/agent-card.json",
  "card": {
    "name": "Invoice agent",
    "description": "Extracts line items from invoices",
    "url": "https://agents.example.com/invoice",
    "version": "1.0",
    "skills": []
  }
}
```

Returns `404` with an empty body if the id is unknown or has expired. Clients then talk A2A to
`endpoint_url` directly; the gateway is not in that path.

## Expiry

An agent registered with `ttl_seconds` disappears from all three endpoints once that many
seconds have passed since it was registered. Expiry is checked when an endpoint is called,
so there is no background clean-up and no delay: the first request after the deadline no
longer sees the agent.

To keep an agent listed with a time to live, have its host register it again before the
deadline. Because registration is first-writer-wins, the host must `DELETE` the agent and then
`POST` it, and the agent is missing from the catalogue between the two calls; see
[Duplicate ids](./admin-api.md#duplicate-ids).
