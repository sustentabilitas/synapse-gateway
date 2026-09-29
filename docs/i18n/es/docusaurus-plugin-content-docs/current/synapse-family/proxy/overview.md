---
sidebar_position: 1
title: synapse-proxy
description: synapse-proxy es un sidecar de proxy inverso guiado por configuración que enruta por prefijo de path, inyecta encabezados y campos del cuerpo derivados del contexto y devuelve las respuestas en streaming.
---

`synapse-proxy` es un sidecar de proxy inverso guiado por configuración. Reenvía cada petición
al upstream elegido por el prefijo de path coincidente más largo, puede eliminar ese prefijo,
inyecta encabezados y campos del cuerpo JSON a partir de un **contexto** por proceso (como el
tenant para el que se ejecuta un sandbox), aplica transformaciones a la petición y a la
respuesta, y devuelve la respuesta del upstream en streaming. Es un binario y una imagen de
Docker distintos del gateway, y ninguno de los dos depende del otro.

## Cuándo usarlo {#when-to-use-it}

Usa el proxy cuando una carga de trabajo tenga que llamar a servicios internos con una
identidad que no debería poder elegir. Tu plano de control fija la identidad una sola vez, a
través del listener de administración del proxy, y el proxy la estampa en cada petición
reenviada, eliminando cualquier valor que el llamante haya intentado enviar. Casos típicos:

- Un sandbox de código o un runtime de agentes que llama a APIs de la plataforma en nombre de
  un tenant.
- Un sidecar que añade encabezados fijos, como un id de usuario o una versión de API, a cada
  llamada.
- Envolver las peticiones en el sobre que espera un upstream, o normalizar sus cuerpos de
  error.

El proxy no autentica a los llamantes ni balancea la carga entre varios upstreams: cada ruta
tiene una única URL de upstream.

## Cómo funciona {#how-it-works}

```text
caller ──► :8787 data plane ──► match route (longest path_prefix, methods)
                                  │  check require_context        → 503 if unbound
                                  │  request steps (inject, wrap, custom)
                                  ▼
                                upstream ──► response steps (error_remap, custom) ──► caller

control plane ──► 127.0.0.1:8788 admin   POST/DELETE /internal/bind  (sets the context)
Prometheus    ──► :9090 metrics          GET /metrics
```

## Inicio rápido {#quick-start}

Guarda esto como `synapse-proxy.toml`. Reenvía `/httpbin/...` al servicio público
[httpbin](https://httpbin.org) y estampa un encabezado `X-Team` a partir del contexto:

```toml
addr = "127.0.0.1:8787"
admin_addr = "127.0.0.1:8788"
metrics_addr = "127.0.0.1:9090"

[context]
static = { team = "my-team" }

[[routes]]
name = "httpbin"
path_prefix = "/httpbin"
upstream = "https://httpbin.org"
strip_prefix = true
require_context = ["team"]
request_steps = [
  { inject = { header = "X-Team", from_context = "team" } },
]
```

Ejecútalo desde una copia local del repositorio:

```bash
SYNAPSE_PROXY_CONFIG_PATH=synapse-proxy.toml cargo run --release -p synapse-proxy
```

El puerto de métricas es `9090`, el mismo que el del gateway; si ejecutas ambos en la misma
máquina, cambia `metrics_addr`. Después envía una petición, vuelve a fijar el contexto y envíala
de nuevo:

```bash
curl -s localhost:8787/httpbin/headers -H 'X-Team: spoofed'
# "X-Team": "my-team"   (el valor del llamante se sobrescribe)

curl -s -X POST localhost:8788/internal/bind \
  -H 'Content-Type: application/json' \
  -d '{"values":{"team":"other-team"},"ttl_seconds":60}'

curl -s localhost:8787/httpbin/headers
# "X-Team": "other-team"   (durante 60 segundos; después vuelve a my-team)
```

Para ejecutar la imagen de Docker, consulta
[Ejecutar el proxy](../../deployment/docker.md#run-the-proxy).

## Próximos pasos {#next-steps}

- [Configuración](./configuration.md): el archivo, las variables de entorno, los tiempos de
  espera y los reintentos.
- [Contexto y transformaciones](./context-and-transforms.md): de dónde sale el contexto y qué
  hace cada transformación.
- [Listeners y endpoints](./listeners-and-endpoints.md): qué sirve cada listener y todos los
  errores que devuelve el proxy.
- [Métricas](./metrics.md): qué recoger y sobre qué alertar.
