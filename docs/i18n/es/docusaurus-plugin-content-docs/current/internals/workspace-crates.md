---
sidebar_position: 2
title: Crates del workspace
description: Los cinco crates del workspace de Cargo de Synapse, qué hace cada uno y cómo dependen unos de otros.
---

Synapse es un único workspace de Cargo con cinco crates bajo `crates/`. Los cinco están
publicados en crates.io, usan la edición 2021 y tienen la licencia MPL-2.0. Las versiones
de esta página son las del `Cargo.toml` de cada crate en `main`.

| Crate | Versión | Nombre de la biblioteca | Binario | Imagen de Docker |
|---|---|---|---|---|
| [`synapse-gateway`](#synapse-gateway) | 0.5.38 | `synapse` | `synapse-gateway` | `sustentabilitas/synapse-gateway` |
| [`synapse-proxy`](#synapse-proxy) | 0.2.21 | `synapse_proxy` | `synapse-proxy` | `sustentabilitas/synapse-proxy` |
| [`synapse-context`](#synapse-context) | 0.1.1 | `synapse_context` | — | — |
| [`synapse-a2a`](#synapse-a2a) | 0.2.2 | `synapse_a2a` | — | — |
| [`synapse-mcp`](#synapse-mcp) | 0.1.4 | `synapse_mcp` | — | — |

## Dependencias entre los crates {#dependencies-between-the-crates}

```text
synapse-gateway ──(feature "server")──► synapse-a2a

synapse-proxy ──► synapse-context ◄── synapse-mcp
```

El gateway y el proxy no dependen el uno del otro. `synapse-context` no tiene dependencias
dentro del workspace, y `synapse-a2a` tampoco. Nada en el workspace depende de `synapse-mcp`:
las aplicaciones que se construyen sobre el proxy lo montan por su cuenta.

## synapse-gateway {#synapse-gateway}

El gateway de LLM, y el tema de la mayor parte de esta documentación. La biblioteca, que se
importa como `synapse`, contiene el tipo `Gateway` y todo lo que hay detrás: el enrutamiento, los
tres carriles, los guardrails, los precios, el registro de costes y las métricas. El binario
`synapse-gateway` la envuelve en el servidor HTTP descrito en la [API HTTP](../reference/http-api.md).

Features de Cargo:

| Feature | Por defecto | Añade |
|---|---|---|
| `server` | Sí | El servidor HTTP de axum, los exportadores de Prometheus y OTLP, y `synapse-a2a`. |
| `ledger-sqlite` | Sí | El destino SQLite del registro de costes. |
| `ledger-postgres` | No | El destino Postgres del registro de costes. |
| `ledger-pubsub` | No | El destino Google Cloud Pub/Sub del registro de costes. |
| `ledger-sns` | No | El destino AWS SNS del registro de costes. |

Para embeber el gateway sin el servidor HTTP, desactiva las features por defecto; consulta
[Embeber Synapse como biblioteca](../guides/embedding-as-library.md).
[Pipeline de una petición](./request-pipeline.md) recorre el código fuente.

- crates.io: [synapse-gateway](https://crates.io/crates/synapse-gateway)
- docs.rs: [synapse-gateway](https://docs.rs/synapse-gateway)

## synapse-proxy {#synapse-proxy}

Un sidecar de proxy inverso guiado por configuración. Reenvía las peticiones a los upstreams
según el prefijo de path coincidente más largo, inyecta encabezados y campos del cuerpo estáticos
o derivados del contexto, ejecuta transformaciones de petición y de respuesta, y deja pasar las
respuestas con streaming. Sirve tres listeners: el plano de datos (`addr`, por defecto
`0.0.0.0:8787`), un listener de administración (`admin_addr`, por defecto `127.0.0.1:8788`) donde
`POST /internal/bind` y `DELETE /internal/bind` establecen y borran el contexto vinculado, y un
listener de métricas (`metrics_addr`, por defecto `0.0.0.0:9090`).

La biblioteca expone `ProxyBuilder`, para registrar transformaciones personalizadas, y reexporta
los tipos de `synapse-context` como `synapse_proxy::context`. Sus métricas están en el
[catálogo de métricas](../reference/metrics-catalogue.md#synapse-proxy), y
[Docker](../deployment/docker.md#run-the-proxy) muestra cómo ejecutar la imagen.

- crates.io: [synapse-proxy](https://crates.io/crates/synapse-proxy)
- docs.rs: [synapse-proxy](https://docs.rs/synapse-proxy)

## synapse-context {#synapse-context}

El almacén de contexto vinculado que comparten `synapse-proxy` y `synapse-mcp`. Un `ContextStore`
contiene un mapa **base** permanente de claves a valores, construido una vez al arrancar (el
proxy combina su configuración estática con las variables de entorno, y ganan las variables de
entorno), y como máximo un **overlay**, establecido en tiempo de ejecución con un tiempo de vida
opcional. `push` reemplaza el overlay, `clear` lo elimina y `resolve` devuelve un
`ResolvedContext`: la base con las claves del overlay por encima mientras el overlay está vigente.
Un overlay caducado se descarta la siguiente vez que se resuelve el almacén, así que el contexto
vuelve a la base sin ninguna tarea en segundo plano. Solo hay una vinculación activa a la vez,
por eso `synapse-mcp` admite una única identidad por proceso. El almacén vive en su propio crate
para que `synapse-proxy` y `synapse-mcp` puedan compartir el mismo tipo sin depender el uno del
otro, y `synapse-proxy` lo reexporta para que las rutas de importación existentes sigan
funcionando.

- crates.io: [synapse-context](https://crates.io/crates/synapse-context)
- docs.rs: [synapse-context](https://docs.rs/synapse-context)

## synapse-a2a {#synapse-a2a}

Un registro en memoria de agentes agent-to-agent (A2A). Proporciona el router de administración
(`POST /internal/a2a/agents`, `DELETE /internal/a2a/agents/{id}`), el router público (el
catálogo, las agent cards y la resolución) y la carga inicial desde `a2a.toml`, que descarga la
agent card de cada agente al arrancar. El registro solo admite inserciones: gana el primer
registro de un id. Las entradas pueden tener un tiempo de vida.

El binario del gateway crea un registro al arrancar e integra ambos routers en su listener de la
API; la [API HTTP](../reference/http-api.md#a2a-agent-registry) documenta los endpoints y
[Claves de configuración](../reference/configuration-keys.md#a2atoml), el archivo de carga
inicial.

- crates.io: [synapse-a2a](https://crates.io/crates/synapse-a2a)
- docs.rs: [synapse-a2a](https://docs.rs/synapse-a2a)

## synapse-mcp {#synapse-mcp}

Un gateway de Model Context Protocol (MCP) bajo demanda, construido sobre `rmcp` 2.2. Los
clientes hablan MCP sobre Streamable HTTP con `/mcp/{server}`, y el gateway reenvía cada llamada
al servidor MCP upstream registrado con ese nombre, añadiendo encabezados de identidad desde un
`ContextStore` compartido según sus reglas configuradas. Si falta una clave de contexto
obligatoria, la llamada falla antes de contactar con ningún upstream. Los servidores upstream se
registran en tiempo de ejecución mediante el router de administración
(`POST /internal/mcp/servers`, `DELETE /internal/mcp/servers/{name}`), opcionalmente con un
tiempo de vida.

El crate proporciona routers y un struct de métricas, no un binario: una aplicación monta
`mcp_gateway_router` y `mcp_admin_router` junto a sus propios listeners y le pasa el
`ContextStore` que comparte con la biblioteca del proxy. Sus métricas están en el
[catálogo de métricas](../reference/metrics-catalogue.md#synapse-mcp) y sus funcionalidades no
admitidas, en [Limitaciones](../reference/limitations-roadmap.md#synapse-mcp).

- crates.io: [synapse-mcp](https://crates.io/crates/synapse-mcp)
- docs.rs: [synapse-mcp](https://docs.rs/synapse-mcp)
