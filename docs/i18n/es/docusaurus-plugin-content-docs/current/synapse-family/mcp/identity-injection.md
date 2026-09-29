---
sidebar_position: 3
title: Inyección de identidad
description: Cómo convierte synapse-mcp el contexto fijado en encabezados de las conexiones MCP con el upstream, y cuándo falla de forma cerrada.
---

En cada llamada, el gateway lee el contexto actual del `ContextStore` compartido y lo convierte
en encabezados de su conexión con el servidor MCP upstream. Tú decides qué claves de contexto se
convierten en qué encabezados.

## Reglas {#rules}

`McpGatewayConfig` contiene una lista de reglas. Cada regla asocia una clave de contexto a un
encabezado:

```rust
use synapse_mcp::{IdentityHeaderRule, McpGatewayConfig};

let config = McpGatewayConfig {
    inject: vec![
        IdentityHeaderRule { context_key: "org".into(), header: "x-org-id".into(), required: true },
        IdentityHeaderRule { context_key: "workspace".into(), header: "x-workspace-id".into(), required: true },
        IdentityHeaderRule { context_key: "user".into(), header: "x-user-id".into(), required: false },
    ],
};
```

Ambos tipos implementan `serde::Deserialize`, así que puedes cargar las reglas desde tu propio
archivo de configuración. En TOML, las mismas reglas son:

```toml
[[inject]]
context_key = "org"
header = "x-org-id"
required = true

[[inject]]
context_key = "workspace"
header = "x-workspace-id"
required = true

[[inject]]
context_key = "user"
header = "x-user-id"
```

`required` vale `false` por defecto. El crate no tiene ninguna noción integrada de identidad:
con una lista `inject` vacía, las llamadas se reenvían sin ningún encabezado de identidad.

## Qué ocurre en una llamada {#what-happens-on-a-call}

Para cada `tools/list` o `tools/call`, antes de cualquier llamada de red:

1. Se busca en el registro el nombre de servidor de `/mcp/{server}`. Un nombre desconocido o
   caducado hace fallar la llamada.
2. Cada regla se aplica al contexto resuelto:
   - una clave fijada establece su encabezado;
   - una clave ausente en una regla `required` **hace fallar la llamada de forma cerrada**, con
     el error MCP `context not bound: missing identity key '<key>'` y, cuando las métricas están
     activadas, cuenta en `broker_identity_injection_failures_total` (consulta el
     [catálogo de métricas](../../reference/metrics-catalogue.md#synapse-mcp));
   - una clave ausente en una regla opcional deja fuera su encabezado.
3. La llamada se envía por una conexión con el upstream que lleva exactamente esos encabezados.

Los encabezados de la petición del propio cliente nunca se reenvían. El gateway abre su propia
conexión con el upstream, así que un cliente que envía `x-org-id` por su cuenta no puede cambiar
la identidad que ve el upstream.

## Una conexión por identidad {#one-connection-per-identity}

`rmcp` fija los encabezados HTTP de una conexión al establecerla; no pueden cambiar en cada
llamada. Por eso el gateway mantiene una conexión con el upstream por servidor e identidad, y
crea una nueva cuando cambia cualquiera de los dos:

- Cuando cambia la identidad fijada, por una nueva fijación o porque ha caducado, la siguiente
  llamada abre una conexión nueva con los nuevos encabezados y cierra la antigua.
- Cuando se vuelve a registrar la URL de un servidor, la siguiente llamada se conecta a la nueva
  URL.

`ContextStore` guarda una sola fijación a la vez, así que el gateway sirve una identidad a la vez
por proceso. Ejecuta un proceso por cada tenant concurrente; consulta
[Hoja de ruta](./roadmap.md).

## Elegir las reglas {#choosing-rules}

Marca como `required` todos los encabezados de los que depende el upstream para la autorización
o el aislamiento de tenants. Una regla opcional sirve para encabezados que el upstream puede
pasar sin ellos, como un id de usuario para logs de auditoría que recurre a una identidad de
servicio.
