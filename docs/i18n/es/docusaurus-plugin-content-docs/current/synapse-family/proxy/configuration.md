---
sidebar_position: 2
title: Configuración del proxy
description: El archivo TOML de synapse-proxy, sus variables de entorno y los tiempos de espera y reintentos hacia el upstream.
---

El proxy lee un archivo TOML al arrancar y unas pocas variables de entorno. No se recarga nada
mientras se ejecuta: cambia el archivo y reinicia el proxy.

## El archivo de configuración {#the-configuration-file}

El proxy carga el archivo indicado por `SYNAPSE_PROXY_CONFIG_PATH`, por defecto
`synapse-proxy.toml` en el directorio de trabajo. El directorio de trabajo de la imagen de
Docker es `/app` e incluye un ejemplo en `/app/synapse-proxy.toml`; monta tu propio archivo
encima. Si el archivo falta o no se puede leer, el proxy se detiene al arrancar.

| Clave | Tipo | Por defecto | Descripción |
|---|---|---|---|
| `addr` | string | `0.0.0.0:8787` | Dirección de escucha del plano de datos. `SYNAPSE_PROXY_ADDR` la sobrescribe. |
| `admin_addr` | string | `127.0.0.1:8788` | Dirección de escucha de administración, para `/internal/bind`. |
| `metrics_addr` | string | `0.0.0.0:9090` | Dirección de escucha de métricas, para `/metrics`. |
| `[context]` | table | empty | De dónde sale el contexto; consulta [Fuentes de contexto](./context-and-transforms.md#context-sources). |
| `[[routes]]` | array of tables | none | Las rutas, descritas más abajo. Sin rutas, todas las peticiones reciben `404`. |

Las claves desconocidas se ignoran, así que una clave mal escrita no tiene ningún efecto y no
avisa. Revisa cada archivo nuevo contra las tablas de esta página.

### Rutas {#routes}

Cada tabla `[[routes]]` reenvía un prefijo de path a un upstream:

| Clave | Tipo | Obligatoria | Por defecto | Descripción |
|---|---|---|---|---|
| `path_prefix` | string | Yes | — | Las peticiones cuyo path empieza por esta cadena coinciden con la ruta. Gana la coincidencia más larga. |
| `upstream` | string | Yes | — | URL base a la que reenviar: esquema, host y un prefijo de path opcional. |
| `name` | string | No | `path_prefix` | Etiqueta usada en métricas y logs. |
| `strip_prefix` | bool | No | `false` | Elimina `path_prefix` del path antes de añadirlo a `upstream`. |
| `methods` | array of strings | No | any | Solo coincide con estos métodos HTTP, por ejemplo `["POST"]`. No distingue mayúsculas de minúsculas. |
| `headers` | table | No | none | Encabezados estáticos que se fijan en cada petición reenviada, antes de ejecutar `request_steps`. |
| `require_context` | array of strings | No | none | Claves de contexto que deben estar fijadas; si no, la petición recibe `503`. |
| `request_steps` | array | No | none | Transformaciones aplicadas a la petición, en orden. |
| `response_steps` | array | No | none | Transformaciones aplicadas a la respuesta del upstream, en orden. |

Los pasos se describen en [Contexto y transformaciones](./context-and-transforms.md). Un paso
no válido, como un `inject` con `header` y `body` a la vez, o un nombre de `transform` que no
se ha registrado, detiene el proxy al arrancar con un error que indica la ruta.

### Ejemplo {#example}

```toml
addr = "0.0.0.0:8787"
admin_addr = "127.0.0.1:8788"
metrics_addr = "0.0.0.0:9090"

[context]
static = { user = "_default" }
env = { org = "TENANT_ORG_ID", workspace = "TENANT_WORKSPACE_ID" }

[[routes]]
name = "orders"
path_prefix = "/v1/orders"
upstream = "http://orders:8080"
strip_prefix = true
require_context = ["org", "workspace"]
headers = { X-Api-Version = "2" }
request_steps = [
  { inject = { header = "X-Tenant-Id",    from_context = "org" } },
  { inject = { header = "X-Workspace-Id", from_context = "workspace" } },
  { inject = { header = "X-User-Id",      from_context = "user" } },
]

[[routes]]
name = "integrations"
path_prefix = "/v1/integrations/call"
upstream = "http://integrations:8080/call"
strip_prefix = true
methods = ["POST"]
require_context = ["org", "workspace"]
request_steps = [
  { wrap = { under = "request", inject = [
      { body = "org",       from_context = "org" },
      { body = "workspace", from_context = "workspace" },
  ] } },
]
response_steps = [
  { error_remap = { when_status = 401, error = "auth_expired" } },
  { error_remap = { when_status = 404, error = "no_connection" } },
]
```

Con este archivo, `GET /v1/orders/42?expand=items` se reenvía a
`http://orders:8080/42?expand=items` con los encabezados de tenant, workspace, usuario y
versión de API fijados. `POST /v1/integrations/call` se reenvía a
`http://integrations:8080/call` con su cuerpo JSON envuelto como
`{"request": <original body>, "org": "...", "workspace": "..."}`.

## Variables de entorno {#environment-variables}

| Variable | Por defecto | Descripción |
|---|---|---|
| `SYNAPSE_PROXY_CONFIG_PATH` | `synapse-proxy.toml` | Ruta al archivo de configuración. |
| `SYNAPSE_PROXY_ADDR` | `addr` from the file | Sobrescribe la dirección de escucha del plano de datos. Se ignora si está vacía. |
| `SYNAPSE_PROXY_UPSTREAM_CONNECT_TIMEOUT_SECS` | `10` | Tiempo permitido para conectar con un upstream. |
| `SYNAPSE_PROXY_UPSTREAM_TIMEOUT_SECS` | `120` | Tiempo permitido para una petición completa al upstream, incluida la lectura de un cuerpo de respuesta en streaming. |
| `SYNAPSE_PROXY_UPSTREAM_SEND_RETRIES` | `2` | Reintentos tras un envío fallido; `0` desactiva los reintentos. |
| `SYNAPSE_PROXY_UPSTREAM_RETRY_BACKOFF_MS` | `200` | Espera antes del primer reintento; se duplica en cada reintento posterior. |
| `RUST_LOG` | `info` | Filtro de logs, con la sintaxis `EnvFilter` de `tracing`. |

Las variables de entorno que nombra la tabla `[context]` también se leen al arrancar; consulta
[Fuentes de contexto](./context-and-transforms.md#context-sources). Un valor que no es un
número válido vuelve al valor por defecto.

## Tiempos de espera y reintentos {#timeouts-and-retries}

El tiempo de espera del upstream cubre todo el intercambio, desde la conexión hasta el último
byte del cuerpo de la respuesta. Una respuesta en streaming que dura más de
`SYNAPSE_PROXY_UPSTREAM_TIMEOUT_SECS` se corta, así que auméntalo para streams de larga
duración.

Cuando una petición falla antes de que el proxy reciba una respuesta del upstream, se
reintenta:

- **Los fallos de conexión** se reintentan para todos los métodos, porque la petición nunca
  llegó al upstream.
- **Los demás fallos de envío**, incluidos los tiempos de espera agotados, solo se reintentan
  para métodos idempotentes: `GET`, `HEAD`, `OPTIONS`, `TRACE`, `PUT` y `DELETE`. Un `POST` o
  un `PATCH` que puede haber llegado al upstream no se envía dos veces.

Con los valores por defecto, los reintentos esperan 200 ms, luego 400 ms, y así sucesivamente.
Cuando se agotan los reintentos, el cliente recibe `502` con `"error": "request_failed"`. Una
respuesta del upstream nunca se reintenta, sea cual sea su estado: un `503` del upstream se
pasa al cliente. Cada reintento y cada fallo definitivo se registran en el log como aviso y se
cuentan en las [métricas](./metrics.md).
