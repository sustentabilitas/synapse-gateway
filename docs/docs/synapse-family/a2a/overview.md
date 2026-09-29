---
sidebar_position: 1
title: synapse-a2a
description: synapse-a2a is an in-memory agent-to-agent (A2A) registry that the Synapse gateway serves on its API port, with admin registration and public discovery.
---

`synapse-a2a` is an in-memory registry of agent-to-agent
([A2A](https://a2a-protocol.org/)) agents. Services that host agents register them, and
clients discover them through a catalogue, fetch their agent cards, and resolve an agent id
to the URL where it serves A2A. The registry stores what it is given and hands it back: it
doesn't proxy A2A traffic or call the agents after startup.

## When to use it

Use the registry when several agent hosts and several clients need one stable place to find
agents by id, and you already run the Synapse gateway. Agent hosts register over HTTP or are
listed in a seed file; clients read the catalogue from the same address as the LLM API.

Because the registry lives in the memory of each gateway process, it suits a single gateway
instance or a set of agents that is known at startup. See
[Running more than one instance](#running-more-than-one-instance).

## How it fits

The gateway binary serves the registry. At startup it creates one registry, seeds it from
`a2a.toml` if the file exists, and merges the admin and public routes into its API listener,
`SYNAPSE_ADDR` (default `0.0.0.0:8080`). There is no separate process or port:

```text
agent host ──POST   /internal/a2a/agents────────────────▶ synapse-gateway :8080
agent host ──DELETE /internal/a2a/agents/{id}───────────▶ same
client     ──GET    /.well-known/a2a-agent-catalog.json─▶ same
client     ──GET    /a2a/agents/{id}/resolve────────────▶ same   ──▶ then calls the agent directly
```

| Method | Path | Purpose |
|---|---|---|
| `POST` | `/internal/a2a/agents` | [Register an agent](./admin-api.md). |
| `DELETE` | `/internal/a2a/agents/{id}` | [Remove an agent](./admin-api.md). |
| `GET` | `/.well-known/a2a-agent-catalog.json` | [List the agents](./discovery.md). |
| `GET` | `/a2a/agents/{id}/.well-known/agent-card.json` | [One agent's card](./discovery.md). |
| `GET` | `/a2a/agents/{id}/resolve` | [One agent's endpoint and card](./discovery.md). |

The same endpoints are listed with the rest of the gateway's API in
[A2A agent registry](../../reference/http-api.md#a2a-agent-registry). The A2A routes don't
read the `x-synapse-*` attribution headers and aren't recorded in the cost ledger or the
metrics.

:::warning
The `/internal/` endpoints have no authentication, and they share a port with the LLM API.
Block them at your ingress for everyone except the services that register agents; see
[The A2A admin endpoints are open](../../operating/security.md#the-a2a-admin-endpoints-are-open).
:::

## Running more than one instance

Each gateway process has its own registry, and nothing is persisted. An agent registered
through the admin API exists only on the instance that received the call, and every
registration is lost on restart. Behind a load balancer, clients would see a different
catalogue depending on which instance answered.

To keep instances consistent, list shared agents in [`a2a.toml`](./static-seed.md) so every
instance seeds the same catalogue at startup, and block the admin endpoints. See also
[Limitations](../../reference/limitations-roadmap.md#operations).

## Using the crate

`synapse-a2a` is also a library. To serve the registry from your own axum application, see
[Crate API](./crate-api.md).
