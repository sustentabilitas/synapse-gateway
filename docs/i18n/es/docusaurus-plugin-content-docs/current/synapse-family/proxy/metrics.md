---
sidebar_position: 5
title: Métricas del proxy
description: Qué exporta synapse-proxy en su listener de métricas y las consultas con las que empezar.
---

El proxy registra métricas de OpenTelemetry y las sirve en formato Prometheus en
`GET /metrics` sobre `metrics_addr` (por defecto `0.0.0.0:9090`). El binario no envía OTLP; una
aplicación que embebe la biblioteca del proxy puede añadir un exportador OTLP con
`Metrics::with_otlp`.

## Qué se registra {#what-is-recorded}

| Nombre en Prometheus | Cuenta o mide |
|---|---|
| `synapse_proxy_requests_total_total` | Cada petición enrutada salvo los cuerpos demasiado grandes (`413`), por `route`, `method`, `status` y `outcome`. |
| `synapse_proxy_request_duration_seconds` | Tiempo hasta que los encabezados de la respuesta estuvieron listos, por `route` y `method`. |
| `synapse_proxy_upstream_retries_total_total` | Envíos al upstream reintentados, por `route` y `reason`. |
| `synapse_proxy_upstream_errors_total_total` | Envíos al upstream que fallaron tras todos los reintentos, por `route` y `reason`. |
| `synapse_proxy_transform_errors_total_total` | Peticiones detenidas por un paso de petición o de respuesta, por `route` y `transform` (`request` o `response`). |

`outcome` es `forwarded`, `no_route`, `context_unbound`, `transform_rejected` o
`upstream_error`, y `route` es `none` cuando ninguna ruta coincidió. Las etiquetas exactas y sus
valores están en el [catálogo de métricas](../../reference/metrics-catalogue.md#synapse-proxy).

:::note
Los contadores terminan de verdad en `_total_total`: el proxy usa la configuración por defecto
del exportador de Prometheus de OpenTelemetry, que añade un segundo sufijo. Los buckets del
histograma de duración están dimensionados para milisegundos, mientras que sus valores están en
segundos, así que usa `_sum / _count` para la latencia, no cuantiles. El
[catálogo](../../reference/metrics-catalogue.md#synapse-proxy) explica ambas cosas.
:::

## Consultas con las que empezar {#queries-to-start-with}

Peticiones que no se reenviaron, por motivo:

```text
sum by (outcome) (rate(synapse_proxy_requests_total_total{outcome!="forwarded"}[5m]))
```

Peticiones rechazadas porque el contexto no está fijado, señal de que el plano de control aún
no ha fijado una identidad o de que la fijación ha caducado:

```text
sum by (route) (rate(synapse_proxy_requests_total_total{outcome="context_unbound"}[5m]))
```

Errores de servidor devueltos por cada upstream, que el proxy deja pasar, junto a los envíos al
upstream que fallaron directamente:

```text
sum by (route, status) (rate(synapse_proxy_requests_total_total{outcome="forwarded", status=~"5.."}[5m]))
sum by (route) (rate(synapse_proxy_upstream_errors_total_total[5m]))
```

Latencia media por ruta:

```text
sum by (route) (rate(synapse_proxy_request_duration_seconds_sum[5m]))
  / sum by (route) (rate(synapse_proxy_request_duration_seconds_count[5m]))
```

## Logs {#logs}

El proxy escribe logs con `tracing` en la salida estándar, filtrados por `RUST_LOG` (por
defecto `info`). Registra sus direcciones de escucha al arrancar, un aviso por cada envío al
upstream reintentado y por cada uno fallido, y una línea cuando empieza a drenar.
