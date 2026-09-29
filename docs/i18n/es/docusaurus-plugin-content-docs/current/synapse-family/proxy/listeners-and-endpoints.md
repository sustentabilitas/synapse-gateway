---
sidebar_position: 4
title: Listeners y endpoints
description: Los tres listeners de synapse-proxy, cómo se emparejan y reenvían las peticiones, los endpoints de administración y de métricas, y todos los errores que devuelve el proxy.
---

El proxy sirve tres listeners a la vez, cada uno en su propia dirección:

| Listener | Clave de configuración | Por defecto | Sirve |
|---|---|---|---|
| Plano de datos | `addr` | `0.0.0.0:8787` | Sondas de salud y tráfico reenviado. |
| Administración | `admin_addr` | `127.0.0.1:8788` | `POST` y `DELETE /internal/bind`. |
| Métricas | `metrics_addr` | `0.0.0.0:9090` | `GET /metrics`. |

El listener de administración no tiene autenticación: quien pueda alcanzarlo puede fijar
cualquier identidad. Mantenlo en `127.0.0.1`, para que solo los procesos del mismo contenedor o
pod puedan llamarlo; publicar el puerto `8788` desde Docker no lo hace accesible. La imagen de
Docker solo declara el puerto `8787`; publica `9090` explícitamente si recoges las métricas
desde fuera del contenedor.

## Plano de datos {#data-plane}

### Sondas de salud {#health-probes}

- `GET /healthz/liveness` siempre devuelve `200`.
- `GET /healthz/readiness` devuelve `200`, o `503` cuando el proxy se está apagando.

El propio proxy responde a estos dos paths; nunca se reenvían.

### Reenvío {#forwarding}

Cualquier otra petición se reenvía:

1. **Emparejar.** El proxy elige la ruta con el `path_prefix` más largo por el que empieza el
   path, entre las rutas cuyos `methods` permiten el método de la petición. El emparejamiento
   es por prefijo de cadena simple, así que `/v1/orders` también coincide con
   `/v1/orders-archive`; termina el prefijo en `/` si eso importa.
2. **Comprobar el contexto.** Todas las claves de `require_context` deben estar fijadas.
3. **Leer el cuerpo.** El cuerpo de la petición se lee completo, hasta 64 MiB, así que los
   cuerpos de las peticiones no se transmiten en streaming.
4. **Transformar.** Se ejecutan en orden los `headers` estáticos y después los `request_steps`.
5. **Reenviar.** La URL es `upstream` (sin `/` final) seguida del path, sin `path_prefix` si
   `strip_prefix` está activado, y de la query string original. Los encabezados del llamante se
   reenvían, salvo los encabezados hop-by-hop (`Connection`, `Keep-Alive`, `Proxy-Connection`,
   `Transfer-Encoding`, `Upgrade`, `TE`, `Trailer`) y `Host`, que se fija para el upstream.
6. **Responder.** Los `response_steps` se ejecutan sobre el estado y los encabezados del
   upstream. Salvo que un paso haya sustituido el cuerpo, el cuerpo del upstream se devuelve en
   streaming a medida que llega, con el estado y los encabezados del upstream menos los
   hop-by-hop.

Consulta [Configuración](./configuration.md#timeouts-and-retries) para los tiempos de espera y
los reintentos.

### Errores {#errors}

Los errores que genera el proxy tienen un cuerpo JSON con `error` y `detail`:

| Estado | `error` | Cuándo |
|---|---|---|
| `400` | `invalid_body` | Un paso de cuerpo necesita JSON y el cuerpo no es JSON válido. |
| `404` | `no_route` | Ninguna ruta coincide con el path y el método. |
| `413` | `body_too_large` | El cuerpo de la petición supera los 64 MiB. |
| `500` | `transform_error` | Una transformación personalizada falló con `TransformError::Internal`. |
| `502` | `request_failed` | El envío al upstream falló, tras los reintentos (consulta [Tiempos de espera y reintentos](configuration.md#timeouts-and-retries)). |
| `503` | `request_failed` | Una clave de `require_context` no está fijada (`"detail": "context not bound"`). |

Una transformación personalizada también puede rechazar una petición con un estado y un `error`
propios; consulta [Transformaciones personalizadas](./context-and-transforms.md#custom-transforms).
Los errores del upstream se pasan sin cambios, salvo que un paso `error_remap` los capture.

## Administración {#admin}

El listener de administración fija la capa superpuesta del contexto; consulta
[Fuentes de contexto](./context-and-transforms.md#context-sources).

### `POST /internal/bind` {#post-internalbind}

```json
{"values": {"org": "acme", "workspace": "team-a"}, "ttl_seconds": 3600}
```

`values` es un objeto de claves y valores de tipo cadena. `ttl_seconds` es opcional; sin él, la
fijación no caduca. La nueva capa superpuesta sustituye a la anterior. Devuelve `204`.

### `DELETE /internal/bind` {#delete-internalbind}

Elimina la capa superpuesta, de modo que el contexto vuelve a los valores construidos al
arrancar. Devuelve `204`.

## Métricas {#metrics}

`GET /metrics` en el listener de métricas devuelve el formato de texto de Prometheus. Consulta
[Métricas](./metrics.md).

## Apagado {#shutdown}

Al recibir `SIGTERM` o `Ctrl-C`, el proxy empieza a responder a `/healthz/readiness` con `503`
para que los balanceadores de carga dejen de enviarle tráfico. Alrededor de un segundo después,
los tres listeners dejan de aceptar conexiones, y el proxy termina cuando han acabado las
peticiones en curso.
