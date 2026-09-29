---
sidebar_position: 5
title: API del crate
description: Sirve el registro de synapse-a2a desde tu propia aplicación axum, siémbralo y registra agentes desde código.
---

El binario del gateway es uno de los usuarios de `synapse-a2a`. El crate proporciona el
registro, los dos routers de axum y la función de siembra, así que puedes servir los mismos
endpoints desde tu propio servicio.

```toml
[dependencies]
synapse-a2a = "0.2"
```

## Servir los endpoints {#serve-the-endpoints}

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

- Comparte un único `Arc<A2aRegistry>` entre los dos routers, para que los registros sean
  visibles para el descubrimiento.
- Los routers están separados, así que puedes servir `a2a_admin_router` en un listener privado
  y solo `a2a_public_router` en el público. El gateway fusiona ambos en su puerto de API.
- `seed_from_path` devuelve un error si el archivo no existe, no se puede leer o no es TOML
  válido; comprueba primero que el archivo existe, como hace el gateway. Los agentes cuya agent
  card no se puede obtener se omiten, como se describe en
  [Semilla estática](./static-seed.md#what-happens-at-startup).
- Para sembrar a partir de agentes que ya tienes en memoria, llama a `seed_agents` con un slice
  de `A2aSeedAgent` y tu propio `reqwest::Client`.

## Registrar agentes desde código {#register-agents-in-code}

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

| Método | Qué hace |
|---|---|
| `try_register(A2aRegistration) -> bool` | Inserta el agente; devuelve `false`, sin cambiar nada, si el id ya está presente. |
| `deregister(&str)` | Elimina un agente; no hace nada si el id no existe. |
| `resolve(&str) -> Option<RegisteredA2aAgent>` | Busca un agente que no haya caducado. |
| `list() -> Vec<RegisteredA2aAgent>` | Todos los agentes que no han caducado. |

`resolve` y `list` descartan las entradas caducadas a medida que las encuentran. El registro es
síncrono y seguro para compartir entre hilos; no se mantiene ningún lock a través de un
`.await`.

También se exportan los tipos de transporte: `RegisterA2aAgentRequest`, `A2aCatalog`,
`A2aCatalogEntry` y `A2aResolveResponse`, todos tipos `serde`, para que un cliente pueda
deserializar con ellos las respuestas del gateway. La API completa está en
[docs.rs](https://docs.rs/synapse-a2a).
