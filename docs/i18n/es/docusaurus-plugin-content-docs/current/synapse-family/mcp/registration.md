---
sidebar_position: 2
title: Registro
description: Registra y elimina los servidores MCP upstream hacia los que enruta synapse-mcp, con caducidad opcional.
---

El gateway solo enruta hacia servidores registrados. Tu plano de control registra los servidores
que puede usar una sesión cuando esta empieza, normalmente con el mismo tiempo de vida que la
fijación de identidad de la sesión, para que ambos caduquen a la vez.

## Endpoints de administración {#admin-endpoints}

`mcp_admin_router` sirve dos endpoints. Móntalo en un listener al que solo pueda llegar tu plano
de control, como el listener de administración de loopback del proxy. No tienen autenticación.

### `POST /internal/mcp/servers` {#post-internalmcpservers}

```bash
curl -s -X POST localhost:8788/internal/mcp/servers \
  -H 'Content-Type: application/json' \
  -d '{"name":"platform","url":"http://platform-mcp:8080/mcp","ttl_seconds":3600}'
```

| Campo | Tipo | Obligatorio | Descripción |
|---|---|---|---|
| `name` | string | Sí | El nombre que usan los clientes en `/mcp/{name}`. |
| `url` | string | Sí | El endpoint MCP Streamable HTTP del servidor upstream. |
| `ttl_seconds` | integer | No | Segundos hasta que caduca el registro. Omítelo para que no caduque. |

Devuelve `204 No Content`. Registrar un nombre que ya existe sustituye su URL y su caducidad. Es
un cambio en caliente: la siguiente llamada a ese nombre se conecta a la nueva URL y cierra la
conexión con la antigua.

### `DELETE /internal/mcp/servers/{name}` {#delete-internalmcpserversname}

Elimina el registro. Devuelve `204 No Content` tanto si el nombre estaba registrado como si no.

## Caducidad {#expiry}

Un registro con `ttl_seconds` deja de resolverse una vez transcurrido ese número de segundos.
Las llamadas a un nombre caducado o desconocido fallan sin ninguna llamada de red, con el error
MCP `unknown or expired mcp server '<name>'`.

El gateway mantiene una conexión abierta por cada servidor registrado. Las conexiones con
servidores que se han eliminado o han caducado se cierran durante la siguiente llamada que llega
a un upstream, a cualquier servidor.

## Registrar desde código {#register-in-code}

No hay archivo semilla. Para registrar servidores al arrancar, llama directamente al registro
antes de servir los routers:

```rust
use std::sync::Arc;
use std::time::Duration;

use synapse_mcp::McpRegistry;

let registry = Arc::new(McpRegistry::new());
registry.register("platform".into(), "http://platform-mcp:8080/mcp".into(), Some(Duration::from_secs(3600)));
```

`register`, `deregister` y `resolve` se comportan como los endpoints anteriores. Pasa el mismo
`Arc<McpRegistry>` a `mcp_admin_router` y a `mcp_gateway_router`.
