---
sidebar_position: 1
title: Métricas
description: Cómo exporta el gateway de Synapse sus métricas synapse_* a Prometheus y a colectores de OpenTelemetry, qué cuentan y con qué consultas y alertas empezar.
---

El gateway registra métricas de cada petición que sirve: tráfico y latencia por ruta, modelo y
carril, uso de tokens, decisiones de enrutamiento, escaneos de guardrails y el estado del
registro de costes. Recopílalas con Prometheus, o envíalas a un colector de OpenTelemetry, para
construir paneles y alertas. El uso y el coste por tenant no son métricas; para eso, consulta
el [registro de costes](../guides/cost-ledger.md).

## Cómo se exportan las métricas {#how-metrics-are-exported}

El gateway registra sus métricas con OpenTelemetry y las exporta de dos formas:

- **Prometheus**, siempre. `GET /metrics` en el listener de métricas, `SYNAPSE_METRICS_ADDR`
  (por defecto `0.0.0.0:9090`), devuelve el formato de texto de Prometheus. `GET /` en el mismo
  puerto devuelve lo mismo. El puerto de la API no sirve métricas.
- **OTLP/HTTP**, cuando `OTEL_EXPORTER_OTLP_ENDPOINT` contiene la URL base de un colector, como
  `http://otel-collector:4318`. El gateway envía a `<endpoint>/v1/metrics` cada 60 segundos, o
  cada `OTEL_METRIC_EXPORT_INTERVAL` milisegundos, con el atributo de recurso `service.name`
  tomado de `OTEL_SERVICE_NAME` (por defecto `synapse-gateway`).

Ambas vías llevan las mismas métricas con los mismos nombres y etiquetas. La línea de log de
arranque `synapse-gateway metrics listening` muestra la dirección de métricas y si OTLP está
activado (`otlp=true`). Consulta [Telemetría](../configuration/environment-variables.md#telemetry)
para ver las variables.

Comprueba qué exporta el gateway:

```bash
curl -s http://localhost:9090/metrics | grep '^synapse_'
```

Una métrica solo aparece después de haberse registrado una vez, así que un gateway recién
arrancado muestra pocas series.

## Formato de exposición {#exposition-format}

La salida de Prometheus se mantiene cerca de lo que produciría una instrumentación de
Prometheus escrita a mano:

- Los nombres de las métricas son exactamente los listados, sin sufijos `_total` ni de unidad
  añadidos, y sin etiquetas `otel_scope_*` ni series `target_info`.
- Las duraciones son histogramas en segundos (series `_bucket`, `_sum` y `_count`). El
  [catálogo de métricas](../reference/metrics-catalogue.md#gateway) lista los límites de los
  buckets.
- Cada métrica conserva como máximo 2,000 combinaciones de etiquetas. A partir de ahí, las
  nuevas combinaciones se agrupan en una única serie con la etiqueta
  `otel_metric_overflow="true"`.

## Qué cubren las métricas {#what-the-metrics-cover}

El [catálogo de métricas](../reference/metrics-catalogue.md) lista cada métrica con su tipo,
unidad, etiquetas y valores de etiqueta.

| Área | Métricas |
|---|---|
| Peticiones de chat | `synapse_requests_total`, `synapse_request_duration_seconds`, `synapse_input_tokens_total`, `synapse_output_tokens_total`, etiquetadas por `route`, `model`, `system` y `lane`. |
| Embeddings | `synapse_embeddings_total`, `synapse_embedding_duration_seconds`. Consulta [Embeddings](../guides/embeddings.md#metrics). |
| Passthrough | `synapse_passthrough_total`, `synapse_passthrough_fallback_total`, para los endpoints passthrough de Gemini y Jev. |
| Jev | `synapse_routing_decisions_total`, `synapse_routing_decision_duration_seconds` para el [router Jev](../guides/jev-router.md#metrics-and-logs); `synapse_jev_extraction_total` para la [extracción híbrida](../guides/jev-lane.md#ledger-and-metrics). |
| Guardrails | `synapse_guard_scans_total`, `synapse_guard_matches_total`, `synapse_guard_scan_duration_seconds`. Consulta [Política de guardrails](../configuration/guardrails-policy.md#metrics). |
| Registro de costes | `synapse_ledger_dropped_total`, `synapse_ledger_errors_total`. Consulta [Entrega](../guides/cost-ledger.md#delivery). |

Tenant, workspace, usuario e hilo no son etiquetas a propósito: sus valores vienen de los
clientes y no están acotados, y cada valor nuevo crearía series nuevas. Usa el registro de
costes para las cifras por tenant.

### Qué cuentan las métricas de peticiones {#what-the-request-metrics-count}

`synapse_requests_total` y sus métricas compañeras cuentan los completados de chat que
produjeron una respuesta:

- Una petición sin streaming se cuenta cuando tiene éxito, con la latencia de toda la petición
  y el modelo del tramo que la sirvió.
- Una petición con streaming se cuenta cuando termina su stream, incluidos los streams que
  fallan después del primer fragmento y los que el cliente abandona. Su latencia corre hasta
  que termina el stream.
- Una petición que falla antes de cualquier respuesta, como un modelo desconocido, un bloqueo
  de guardrails o una cadena en la que fallaron todos los tramos, no se cuenta. Tampoco las
  decisiones del router Jev, los embeddings ni las llamadas passthrough, que tienen sus propias
  métricas.
- Una petición de extracción híbrida se cuenta una vez por su respuesta de Jev y una vez por
  cada extracción.

No hay ninguna métrica para los completados de chat fallidos. Mide las tasas de error en el
balanceador de carga, el ingress o el service mesh que tengas delante del gateway.

## Consultas {#queries}

Tasa de peticiones por ruta y carril:

```text
sum by (route, lane) (rate(synapse_requests_total[5m]))
```

Latencia del percentil 95 por ruta:

```text
histogram_quantile(0.95, sum by (le, route) (rate(synapse_request_duration_seconds_bucket[5m])))
```

Tokens de salida por segundo por modelo que sirve:

```text
sum by (model) (rate(synapse_output_tokens_total[5m]))
```

Qué modelos sirven cada ruta. La etiqueta `model` indica el tramo que sirvió cada petición, así
que el tráfico en un modelo distinto del primer tramo de la ruta significa que la cadena está
recurriendo al fallback:

```text
sum by (route, model) (rate(synapse_requests_total[1h]))
```

Proporción de decisiones del router Jev que recurrieron a `default_tier`:

```text
sum by (route) (rate(synapse_routing_decisions_total{outcome=~"timeout|error|low_confidence"}[15m]))
  / sum by (route) (rate(synapse_routing_decisions_total[15m]))
```

## Alertas para empezar {#alerts-to-start-with}

| Alertar sobre | Expresión | Por qué |
|---|---|---|
| Filas del registro perdidas | `increase(synapse_ledger_dropped_total[10m]) > 0` | La cola del registro se desbordó y el uso no se registró. |
| Destino del registro fallando | `sum by (backend) (increase(synapse_ledger_errors_total[10m])) > 0` | Un destino está rechazando escrituras; las filas que se le envían se pierden. |
| Enrutamiento Jev degradado | La proporción de decisiones anterior `> 0.2` | Las peticiones acaban en `default_tier` sin una decisión real. |
| Errores de passthrough | `sum by (provider) (rate(synapse_passthrough_total{status="error"}[5m])) > 0` | Los clientes de los SDK de Gemini o Jev están recibiendo errores. |
| Bloqueos de guardrails | `sum by (policy) (rate(synapse_guard_scans_total{outcome="block"}[15m]))` por encima de tu línea base | Un pico puede indicar un ataque, o una política que bloquea tráfico legítimo. |
| Gateway caído | `up{job="synapse-gateway"} == 0` | Prometheus no puede recopilar las métricas del gateway. |

## Gateways embebidos {#embedded-gateways}

Un gateway construido con `Gateway::builder()` no registra ninguna métrica a menos que le pases
un `GatewayMetrics`. Consulta
[Lo que el builder deja fuera](../guides/embedding-as-library.md#what-the-builder-leaves-out).
