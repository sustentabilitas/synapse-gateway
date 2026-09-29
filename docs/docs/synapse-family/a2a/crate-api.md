---
sidebar_position: 5
title: Crate API
description: Serve the synapse-a2a registry from your own axum application, seed it, and register agents in code.
---

The gateway binary is one user of `synapse-a2a`. The crate provides the registry, the two
axum routers and the seeding function, so you can serve the same endpoints from your own
service.

```toml
[dependencies]
synapse-a2a = "0.2"
```

## Serve the endpoints

```rust
use std::sync::Arc;

use synapse_a2a::{a2a_admin_router, a2a_public_router, seed_from_path, A2aRegistry};

async fn app() -> anyhow::Result<axum::Router> {
    let registry = Arc::new(A2aRegistry::new());

    let seed = "config/a2a.toml";
    if std::path::Path::new(seed).exists() {
        seed_from_path(&registry, seed).await?;
    }

    Ok(axum::Router::new()
        .merge(a2a_admin_router(registry.clone()))
        .merge(a2a_public_router(registry)))
}
```

- Share one `Arc<A2aRegistry>` between the two routers, so registrations are visible to
  discovery.
- The routers are separate, so you can serve `a2a_admin_router` on a private listener and
  only `a2a_public_router` on the public one. The gateway merges both into its API port.
- `seed_from_path` returns an error if the file is missing, can't be read or isn't valid TOML;
  check that the file exists first, as the gateway does. Agents whose card can't be fetched
  are skipped, as described in [Static seed](./static-seed.md#what-happens-at-startup).
- To seed from agents you already have in memory, call `seed_agents` with a slice of
  `A2aSeedAgent` and your own `reqwest::Client`.

## Register agents in code

```rust
use std::time::Duration;

use synapse_a2a::{A2aRegistration, A2aRegistry};

fn register(registry: &A2aRegistry, card: serde_json::Value) -> bool {
    registry.try_register(A2aRegistration {
        id: "invoice-agent".into(),
        name: "Invoice agent".into(),
        description: "Extracts line items from invoices".into(),
        endpoint_url: "https://agents.example.com/invoice".into(),
        card_url: "https://agents.example.com/invoice/.well-known/agent-card.json".into(),
        tags: vec!["finance".into()],
        card,
        ttl: Some(Duration::from_secs(3600)),
    })
}
```

| Method | Does |
|---|---|
| `try_register(A2aRegistration) -> bool` | Inserts the agent; returns `false`, and changes nothing, if the id is already present. |
| `deregister(&str)` | Removes an agent; does nothing if the id is absent. |
| `resolve(&str) -> Option<RegisteredA2aAgent>` | Looks up an unexpired agent. |
| `list() -> Vec<RegisteredA2aAgent>` | Every unexpired agent. |

`resolve` and `list` drop expired entries as they go. The registry is synchronous and safe to
share between threads; no lock is held across an `.await`.

The wire types are exported too: `RegisterA2aAgentRequest`, `A2aCatalog`, `A2aCatalogEntry`
and `A2aResolveResponse`, all `serde` types, so a client can deserialise the gateway's
responses with them. The full API is on [docs.rs](https://docs.rs/synapse-a2a).
