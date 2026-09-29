---
slug: opentelemetry-metrics
title: "Métricas de OpenTelemetry en Synapse"
authors: [rajwilkhu]
tags: [observability]
date: 2026-09-28T09:30
description: Synapse 0.5.38 registra sus métricas synapse_* con opentelemetry-rust, las sirve en formato Prometheus, puede enviarlas por OTLP e incluye un panel de Grafana para visualizarlas.
---

Desde la versión 0.5.38, el gateway Synapse registra sus métricas con
[opentelemetry-rust](https://github.com/open-telemetry/opentelemetry-rust) en lugar del crate
`metrics`. Tu scrape de Prometheus sigue funcionando con los mismos nombres de series y
etiquetas, salvo dos cambios deliberados que se explican más abajo, y ahora basta con definir
una variable de entorno para enviar esas mismas métricas a un colector de OpenTelemetry.

Esta entrada explica por qué hicimos el cambio, qué aspecto tiene la exposición, qué cubren las
métricas y cómo poner en marcha el panel de Grafana de ejemplo.

<!-- truncate -->

## Por qué opentelemetry-rust {#why-opentelemetry-rust}

El gateway era la excepción dentro de su propio workspace. `synapse-proxy` y `synapse-mcp` ya
registraban a través de instrumentos de OpenTelemetry, mientras que el gateway usaba el crate
`metrics` y su propio exportador. Pasar el gateway a opentelemetry-rust 0.32 pone los tres
crates en un mismo pipeline: instrumentos creados a partir de un `Meter` de OpenTelemetry,
exportados por el exportador de Prometheus de OpenTelemetry, con OTLP disponible a su lado. (El
proxy conserva la nomenclatura por defecto del exportador; consulta el
[catálogo de métricas](/docs/reference/metrics-catalogue/).)

Esto importa sobre todo cuando embebes el gateway. Un servicio de Rust que construye un
`Gateway` en código le pasa ahora un `GatewayMetrics` creado a partir de un meter de su propio
`MeterProvider`, y los instrumentos del gateway acaban allí donde exporte ese provider. El valor
por defecto es un no-op, así que un gateway embebido no registra nada hasta que lo activas; un
recorder global de `metrics` en el proceso anfitrión ya no recibe las series del gateway. Con la
feature `server`, `synapse::telemetry::install` construye exactamente los exportadores que usa el
binario. Consulta
[Embeber Synapse como biblioteca](/docs/guides/embedding-as-library/#what-the-builder-leaves-out).

## Prometheus por defecto, OTLP cuando lo pides {#prometheus-by-default-otlp-when-you-ask}

El binario siempre sirve texto de Prometheus en su listener de métricas, `SYNAPSE_METRICS_ADDR`,
que por defecto es `0.0.0.0:9090`, en `GET /metrics`. El puerto de la API no sirve métricas.

```bash
curl -s http://localhost:9090/metrics | grep '^synapse_'
```

Define `OTEL_EXPORTER_OTLP_ENDPOINT` con la URL base de un colector, como
`http://otel-collector:4318`, y el gateway también envía las mismas métricas por OTLP/HTTP a
`<endpoint>/v1/metrics`, cada 60 segundos por defecto, etiquetadas con `service.name` a partir de
`OTEL_SERVICE_NAME` (por defecto `synapse-gateway`). Ambas vías llevan los mismos nombres y
etiquetas. La línea de arranque `synapse-gateway metrics listening` te indica si OTLP está
activado. Las variables de [Telemetría](/docs/configuration/environment-variables/#telemetry)
aparecen junto al resto de la configuración.

## Una exposición que parece escrita a mano {#an-exposition-that-looks-hand-written}

El exportador de Prometheus de OpenTelemetry añade por defecto cosas que habrían roto los paneles
existentes: un segundo `_total` en los contadores, sufijos de unidad, etiquetas `otel_scope_*` y
una serie `target_info`. El gateway desactiva todo eso, así que los nombres salen exactamente
como los enumera el catálogo, por ejemplo `synapse_requests_total` y
`synapse_request_duration_seconds`.

Dos cosas sí cambiaron, ambas de forma deliberada:

- **Las duraciones son histogramas.** Las métricas `*_duration_seconds` exponen ahora series
  `_bucket`, `_sum` y `_count`, con buckets de 5 ms a 120 segundos, en lugar de summaries. Las
  consultas sobre `{quantile=...}` pasan a `histogram_quantile(...)` sobre `_bucket`, y ahora
  puedes agregar la latencia entre instancias, algo que los summaries nunca permitieron.
- **La cardinalidad está limitada.** Cada métrica conserva como máximo 2,000 combinaciones de
  etiquetas, el valor por defecto del SDK de OpenTelemetry. A partir de ahí, las combinaciones
  nuevas se agrupan en una sola serie etiquetada `otel_metric_overflow="true"`.

## Qué cubren las métricas {#what-the-metrics-cover}

El [catálogo de métricas](/docs/reference/metrics-catalogue/) enumera cada instrumento con su
tipo y sus etiquetas. En resumen, el gateway cuenta:

- **Peticiones de chat:** peticiones, latencia y tokens de entrada y salida, etiquetados por
  `route`, el `model` que sirve la petición, `lane` y `system`, la familia de proveedor en el
  vocabulario `gen_ai.system` de OpenLLMetry (`vertexai`, `openai`, `dashscope`, `oai_compat`).
- **Embeddings y passthrough:** peticiones de embeddings y su latencia, y llamadas passthrough
  por acción y estado para los endpoints passthrough de Gemini y Jev, además de los fallbacks de
  modelo del passthrough de Gemini.
- **Jev:** decisiones de enrutamiento por nivel y resultado, latencia de la decisión y
  respuestas de extracción híbrida.
- **Guardrails:** escaneos por resultado, coincidencias por escáner y severidad, y latencia de
  los escaneos.
- **El registro de costes:** filas descartadas porque la cola estaba llena y filas que un
  destino no pudo escribir.

Faltan dos cosas a propósito. Los tenants no son etiquetas, porque sus valores vienen de los
clientes y no están acotados; el uso y el coste por tenant viven en el
[registro de costes](/docs/guides/cost-ledger/), que puedes consultar directamente. Y no hay
ninguna métrica de chat completions fallidas: las métricas de peticiones cuentan las peticiones
que produjeron una respuesta, así que mide las tasas de error en el balanceador de carga o en el
service mesh que haya delante del gateway.
[Qué cuentan las métricas de peticiones](/docs/operating/metrics/#what-the-request-metrics-count)
detalla los casos límite, como los streams abandonados.

El gateway solo registra métricas. No crea spans de trazas de OpenTelemetry; los logs pasan por
`tracing`, filtrados con `RUST_LOG`. Consulta [Logging](/docs/operating/logging/#tracing).

## Un panel para empezar {#a-dashboard-to-start-from}

El repositorio incluye un panel de Grafana de ejemplo,
[synapse-gateway-dashboard.json](pathname:///dashboards/synapse-gateway-dashboard.json). Muestra
el tráfico por ruta, carril y modelo; percentiles de latencia; tasas de tokens; el estado del
registro de costes; y embeddings. Los gráficos de latencia son los que arregló el paso a
histogramas: consultan `histogram_quantile` sobre series `_bucket`, que los antiguos summaries
nunca producían, así que antes se quedaban vacíos.

Dos cosas que conviene comprobar antes de importarlo: cada gráfico se refiere a su fuente de
datos de Prometheus por el UID `prometheus`, y cada consulta filtra por una variable `$job` cuyo
valor por defecto es `synapse-gateway`. La página del
[panel de Grafana](/docs/operating/grafana-dashboard/) explica cómo importarlo a mano,
aprovisionarlo con Docker Compose y cargarlo en Kubernetes mediante el sidecar de
kube-prometheus-stack. Su fila Resilience se queda vacía, porque los tramos de chat no tienen
reintentos ni circuit breakers de los que informar.

## Por dónde empezar {#where-to-start}

Apunta Prometheus al puerto 9090, importa el panel y añade las alertas iniciales de
[Métricas](/docs/operating/metrics/#alerts-to-start-with): las filas perdidas del registro de
costes, un destino del registro que falla y el enrutamiento de Jev degradado son problemas de los
que ningún error de cliente te va a avisar.
