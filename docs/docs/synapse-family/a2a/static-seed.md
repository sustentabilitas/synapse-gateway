---
sidebar_position: 2
title: Static seed
description: Seed the A2A registry at gateway startup from a2a.toml, and how the gateway fetches each agent's card.
---

List agents in a seed file when they should be in the catalogue from the moment the gateway
starts, on every instance, without anyone calling the admin API.

## The seed file

The gateway reads the file named by `SYNAPSE_A2A_PATH`, by default `config/a2a.toml`
relative to its working directory (`/app/config/a2a.toml` in the Docker image). Each agent is
an `[[a2a_agents]]` table:

```toml
[[a2a_agents]]
id = "invoice-agent"
name = "Invoice agent"
description = "Extracts line items from invoices"
endpoint_url = "https://agents.example.com/invoice"
card_url = "https://agents.example.com/invoice/.well-known/agent-card.json"
tags = ["finance"]

[[a2a_agents]]
id = "ghg-emissions"
name = "GHG emissions"
description = "Estimates greenhouse gas emissions"
endpoint_url = "https://agents.example.com/ghg"
card_url = "https://agents.example.com/ghg/.well-known/agent-card.json"
tags = ["ghg", "emissions"]
ttl_seconds = 86400
```

`id`, `name`, `description`, `endpoint_url` and `card_url` are required; `tags` and
`ttl_seconds` are optional. Unlike a registration through the admin API, the seed file has no
`card` field: the gateway fetches the card. Every key is described in
[Configuration keys](../../reference/configuration-keys.md#a2atoml).

## What happens at startup

- **No file:** the registry starts empty and the gateway logs that it found no seed file.
- **A file that can't be read or parsed:** the gateway doesn't start.
- **Otherwise,** the gateway handles the agents one after another, before it starts serving:
  1. It fetches the card with `GET card_url`, allowing 15 seconds per attempt.
  2. Connection errors, timeouts, `5xx` and `429` responses are retried up to twice, with
     exponential backoff starting at 200 ms. Any other non-`2xx` status, or a body that
     isn't JSON, fails at once.
  3. If the card can't be fetched, the agent is skipped with a warning and the gateway
     carries on without it. An agent host that is still starting can't stop the gateway
     from coming up.
  4. Otherwise the agent is registered with the fetched card.

The card is fetched once. The registry doesn't refresh it later, so an agent whose card
changes needs a gateway restart, or a `DELETE` and `POST` through the
[admin API](./admin-api.md).

Because seeding is sequential and blocks startup, an unreachable agent host adds up to about
45 seconds to startup. Keep the file to agents whose hosts are usually up.

## Registration semantics

Registration is **first-writer-wins**, for the seed file and the admin API alike:

- An `id` that is already registered is not replaced. A duplicate `id` in the seed file is
  skipped with a warning, and the first entry stays.
- To replace an agent, `DELETE` it and register it again.

## Expiry

With `ttl_seconds`, a seeded agent leaves the catalogue that many seconds after the gateway
started. Leave it out for agents that should stay for the life of the process. See
[Expiry](./discovery.md#expiry).
