---
sidebar_position: 3
title: Contexto y transformaciones
description: Cómo construye synapse-proxy el contexto de cada petición, las transformaciones inject, wrap y error_remap que lo usan, y las transformaciones personalizadas en Rust.
---

El **contexto** es un conjunto de claves y valores de tipo cadena, como `org = "acme"`, que el
proxy puede estampar en las peticiones reenviadas. Las transformaciones son los pasos que hacen
ese estampado, reestructuran los cuerpos de las peticiones y normalizan los errores del
upstream.

## Fuentes de contexto {#context-sources}

El contexto tiene dos capas.

La **base** se construye una sola vez al arrancar, a partir de la tabla `[context]`:

- `static` fija valores literales: `static = { user = "_default" }`.
- `env` asocia una clave de contexto a la variable de entorno de la que se lee:
  `env = { org = "TENANT_ORG_ID" }`. Cuando una clave está en ambas, gana la variable de
  entorno. Una variable sin definir o vacía deja la clave fuera, o con su valor de `static`.

La **capa superpuesta** (overlay) se envía en tiempo de ejecución a través del listener de
administración. Se sitúa encima de la base: sus claves ganan, y las claves de la base que no
menciona siguen visibles.

```bash
curl -s -X POST localhost:8788/internal/bind \
  -H 'Content-Type: application/json' \
  -d '{"values":{"org":"acme","workspace":"team-a"},"ttl_seconds":3600}'
```

- Solo hay una capa superpuesta a la vez. Cada `POST` sustituye por completo a la anterior, así
  que envía todas las claves que quieras fijar, no solo las que han cambiado.
- Con `ttl_seconds`, la capa superpuesta caduca tras ese número de segundos y el contexto vuelve
  a la base. Sin él, dura hasta que se sustituye o se borra.
- `DELETE /internal/bind` elimina la capa superpuesta de inmediato.

El contexto se resuelve en cada petición, así que una nueva fijación se aplica a la siguiente
petición. El almacén es el `ContextStore` del crate `synapse-context`; consulta
[Crates del workspace](../../internals/workspace-crates.md#synapse-context).

## Exigir contexto {#requiring-context}

Enumera en `require_context` las claves sin las que una ruta no puede funcionar. Si falta
alguna, tanto en la base como en la capa superpuesta, el proxy responde sin contactar con el
upstream:

```http
HTTP/1.1 503 Service Unavailable

{"error":"request_failed","detail":"context not bound"}
```

## Pasos de petición {#request-steps}

Los `request_steps` se ejecutan en orden, después de los `headers` estáticos de la ruta. Un paso
que falla detiene la petición; consulta [Errores](./listeners-and-endpoints.md#errors).

### inject {#inject}

Fija un encabezado o un campo del cuerpo JSON, a partir de una clave de contexto o de una
constante:

```toml
{ inject = { header = "X-Tenant-Id", from_context = "org" } }
{ inject = { header = "X-User-Id",   const = "_default" } }
{ inject = { body = "tenant",        from_context = "org" } }
{ inject = { body = "params.context.user", from_context = "user" } }
```

- Indica exactamente uno de `header` y `body`, y exactamente uno de `from_context` y `const`.
- `body` es una ruta con puntos. Los objetos que falten a lo largo de la ruta se crean, y un
  valor intermedio que no sea un objeto se sustituye.
- `const` puede ser cualquier valor TOML. En un cuerpo conserva su tipo; en un encabezado, una
  cadena se envía tal cual y cualquier otra cosa como su texto JSON.
- Un valor inyectado sobrescribe lo que el llamante haya enviado en ese encabezado o campo.

Cuando una clave de `from_context` no está fijada:

- **Los destinos de encabezado fallan de forma segura.** El proxy elimina el encabezado, para
  que un valor enviado por el llamante no pueda colarse en lugar de la identidad que falta.
- **Los destinos de cuerpo no se tocan.** Se reenvía el valor del llamante, si lo hay. Usa
  `require_context` para detener estas peticiones.

:::warning
Cualquier clave de contexto que inyectes como identidad, como un id de tenant o de usuario,
debería figurar también en el `require_context` de la ruta. Es la única protección para los
destinos de cuerpo, y convierte una fijación ausente en un `503` explícito en lugar de una
petición sin identidad.
:::

### wrap {#wrap}

Anida todo el cuerpo JSON de la petición bajo una clave y luego inyecta campos hermanos:

```toml
{ wrap = { under = "request", inject = [
    { body = "org",       from_context = "org" },
    { body = "workspace", from_context = "workspace" },
] } }
```

Un cuerpo `{"method":"GET","path":"/p"}` se reenvía como
`{"request":{"method":"GET","path":"/p"},"org":"acme","workspace":"team-a"}`. Las entradas de
`inject` admiten las mismas claves que el paso `inject`.

### Tratamiento del cuerpo {#body-handling}

Un paso de cuerpo (`inject` con `body`, o `wrap`) necesita un cuerpo JSON. Un cuerpo vacío
cuenta como `{}`; cualquier otra cosa que no sea JSON válido se rechaza con `400` y
`"error": "invalid_body"`. Las peticiones que ningún paso de cuerpo toca se reenvían byte a
byte, sean JSON o no.

Tras un paso de cuerpo, el proxy vuelve a serializar el JSON y descarta los encabezados
`Content-Length` y `Transfer-Encoding` del llamante, para que el upstream reciba una longitud
que coincida con el nuevo cuerpo.

## Pasos de respuesta {#response-steps}

Los `response_steps` se ejecutan en orden una vez que el upstream ha respondido.

### error_remap {#error_remap}

Cuando el estado del upstream es igual a `when_status`, sustituye el cuerpo de la respuesta por
un objeto de error normalizado. El código de estado se conserva:

```toml
{ error_remap = { when_status = 401, error = "auth_expired" } }
{ error_remap = { when_status = 404, error = "no_connection", detail = "resource not found" } }
```

La primera línea convierte cualquier cuerpo de un `401` en `{"error":"auth_expired"}`; la
segunda añade `"detail"`. Una respuesta reasignada se envía como JSON, sin los encabezados del
upstream. Las respuestas que ningún paso reasigna se devuelven al llamante en streaming, sin
cambios.

## Transformaciones personalizadas {#custom-transforms}

Para hacer algo que los pasos integrados no pueden, escribe una transformación en Rust y
ejecuta el proxy como biblioteca. Implementa `RequestTransform` o `ResponseTransform` (ambos
usan `async-trait`) y regístrala por nombre en un `ProxyBuilder`:

```rust
use std::sync::Arc;

use async_trait::async_trait;
use synapse_proxy::config::Config;
use synapse_proxy::context::ResolvedContext;
use synapse_proxy::transform::{ProxyRequest, RequestTransform, TransformError};
use synapse_proxy::ProxyBuilder;

struct TenantTag;

#[async_trait]
impl RequestTransform for TenantTag {
    async fn apply(&self, ctx: &ResolvedContext, req: &mut ProxyRequest) -> Result<(), TransformError> {
        req.set_header("x-tenant-tag", ctx.get("org").unwrap_or("unknown"));
        Ok(())
    }
}

fn data_plane(config: Config) -> anyhow::Result<axum::Router> {
    synapse_proxy::build_router_from_config(
        ProxyBuilder::from_config(config).request_transform("tenant-tag", Arc::new(TenantTag)),
    )
}
```

Después úsala en la configuración:

```toml
request_steps = [ { transform = "tenant-tag" } ]
```

- `ProxyRequest` te da el método, el path, la query y los encabezados, además de `set_header`,
  `remove_header` y `body_json_mut`. `ProxyResponse` te da el estado y los encabezados, y
  `replace_body`.
- Devuelve `TransformError::Reject { status, error, detail }` para responder al llamante con
  ese estado y un cuerpo `{"error", "detail"}`, o `TransformError::Internal(message)` para un
  `500`.
- `build_router_from_config` construye solo el plano de datos, con métricas que no se exportan.
  Para servir también los listeners de administración y de métricas, llama a
  `ProxyBuilder::build` y ensambla los routers como lo hace el binario en
  [`main.rs`](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-proxy/src/main.rs).

La API está documentada en [docs.rs](https://docs.rs/synapse-proxy).
