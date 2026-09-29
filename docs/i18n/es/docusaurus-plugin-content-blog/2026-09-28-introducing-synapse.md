---
slug: introducing-synapse
title: "Presentamos Synapse: un gateway de LLM que conserva la potencia nativa"
authors: [rajwilkhu]
tags: [release, vertex, routing]
date: 2026-09-28T10:00
description: Synapse es un gateway de LLM de código abierto escrito en Rust que habla la API de OpenAI con tus clientes y conserva por detrás las funcionalidades nativas de Vertex AI, el enrutamiento con Jev y la contabilidad de costes por tenant.
---

Synapse es un gateway de LLM de código abierto escrito en Rust. Tus clientes envían peticiones
estándar `POST /v1/chat/completions` de OpenAI, y Synapse enruta cada una a través de una cadena
de fallback de proveedores definida en la configuración, registra cuánto ha costado y devuelve
una respuesta con forma de OpenAI.

Lo que lo diferencia es aquello a lo que se niega a renunciar. La caché de contexto de Vertex AI,
los medios en Cloud Storage y los esquemas de respuesta estrictos sobreviven al viaje, y un
modelo de enrutamiento puede decidir cuánta capacidad de modelo, y cuánto razonamiento, merece
cada petición. Esta entrada explica por qué lo creamos, cómo está construido y hacia dónde va.

<!-- truncate -->

## Intentamos no tener que escribirlo {#we-tried-not-to-write-one}

Empezamos donde empieza la mayoría de los equipos: poner un proxy genérico compatible con OpenAI
delante de todos los proveedores y seguir adelante. Ese enfoque llega a Gemini a través de un
adaptador con forma de OpenAI, y es en el adaptador donde desaparecen las funcionalidades de las
que dependemos. No hay ningún campo de OpenAI para un recurso `cachedContents` de Vertex ni para
un vídeo `gs://`, y un esquema `response_format` de OpenAI solo se convierte en un
`responseSchema` estricto de Vertex si el adaptador lo traduce, así que una capa de traducción o
bien descarta estas funcionalidades, o bien nunca llega a conocerlas.

Queríamos enrutamiento y fallback entre varios proveedores, y queríamos las funcionalidades
nativas de Vertex, no una cosa o la otra. Por eso Synapse mantiene un carril nativo dedicado
para Vertex, junto al carril compatible con OpenAI que atiende todo lo demás, y mantiene el
código lo bastante pequeño como para que el código de enrutamiento, fallback, registro de costes
y métricas sea tuyo para leerlo, ejecutarlo y embeberlo.

## Tres carriles, un endpoint {#three-lanes-one-endpoint}

Todas las peticiones de chat van al mismo endpoint. Solo el cuerpo de la petición decide cuál de
los tres carriles la atiende, de modo que un cliente activa las funcionalidades nativas
añadiendo un bloque de extensión, sin una segunda API que aprender. La
[visión general de la arquitectura](/docs/overview/architecture/) recorre el flujo completo de
una petición.

- **El carril estándar** llama a los proveedores a través del crate
  [`genai`](https://crates.io/crates/genai). OpenAI, Qwen (DashScope) y vLLM, Ollama o TGI
  autoalojados mediante el proveedor `oai_compat` pueden aparecer en una misma cadena de
  fallback, y también Vertex, sin sus funcionalidades exclusivas del carril nativo.
- **El carril Vertex nativo** recibe cualquier petición cuyo bloque `vertex` lleve
  `cached_content`, `response_schema`, `thinking_config` o una URI de medios `gs://`. Synapse
  traduce los mensajes de OpenAI al formato de Vertex y llama directamente al endpoint
  `:streamGenerateContent` de Vertex AI, almacenando el stream en búfer para los clientes que no
  lo pidieron. Solo participan los tramos `vertex` de la ruta; si una ruta no tiene ninguno, la
  petición falla con `native_feature_unsupported` en lugar de perder las funcionalidades en
  silencio. Consulta la [guía de Vertex nativo](/docs/guides/native-vertex/).
- **El carril Jev** envía preguntas tipadas a TypeSafe System One (Jev), que devuelve decisiones
  estructuradas en lugar de texto libre. También puede evaluar y después extraer en una sola
  llamada. Consulta la [guía del carril Jev](/docs/guides/jev-lane/).

Una petición nativa es como cualquier otro chat completion, más un bloque `vertex`:

```json
{
  "model": "gemini-flash",
  "messages": [{ "role": "user", "content": "Summarise the video." }],
  "vertex": {
    "media_uris": ["gs://cloud-samples-data/video/animals.mp4"],
    "response_schema": { "type": "object", "properties": { "summary": { "type": "string" } } }
  }
}
```

Aquí `gemini-flash` es un alias de ruta para `gemini-3.5-flash-lite` en Vertex, como en la
[guía de inicio rápido](/docs/get-started/quickstart/).

## Enrutamiento por dificultad {#routing-by-difficulty}

Los carriles deciden *cómo* llega una petición a un proveedor. Las rutas deciden *qué* tramos
prueba. Una ruta estática es una lista ordenada de tramos que se prueban hasta que uno tiene
éxito. Una ruta `strategy = "jev"`, en cambio, declara niveles de dificultad, descritos por el
tipo de trabajo para el que sirven. Para cada petición, el router Jev pregunta a Jev lo exigente
que es la conversación y si necesita razonamiento paso a paso, y después la sirve desde el nivel
correspondiente con el esfuerzo de razonamiento adecuado. La respuesta indica qué ha ocurrido en
`x-synapse-routing`, `x-synapse-tier` y encabezados relacionados.

Una entrada complementaria, [El enrutamiento con Jev, explicado](/blog/jev-routing-explained/),
cubre la decisión en detalle, y la [guía del router Jev](/docs/guides/jev-router/) es la
referencia.

## La base, no un complemento {#the-baseline-not-an-add-on}

Algunas cosas que consideramos imprescindibles vienen con cada despliegue:

- **Streaming real.** En los carriles estándar y Vertex nativo, Synapse siempre hace streaming
  desde el proveedor, así que los clientes con `stream: true` reciben server-sent events token a
  token. Los clientes sin streaming reciben el resultado en búfer y, en el carril estándar,
  conservan toda la cadena de fallback. Consulta
  [Streaming y llamadas a herramientas](/docs/guides/streaming-and-tools/).
- **Llamadas a herramientas en los carriles estándar y Vertex nativo.** El carril nativo también
  respeta `tool_choice` mediante `toolConfig` de Vertex; el carril estándar reenvía las
  herramientas pero descarta `tool_choice`.
- **Un registro de costes que es tuyo.** Cada petición se atribuye a un tenant a partir del
  encabezado `x-synapse-tenant` y se valora según tu `pricing.toml` en SQLite o Postgres, con
  distribución opcional a Google Cloud Pub/Sub y AWS SNS. Consulta la
  [guía del registro de costes](/docs/guides/cost-ledger/).
- **Métricas.** Métricas `synapse_*` de OpenTelemetry, servidas en formato Prometheus en el
  puerto 9090 y, opcionalmente, enviadas por OTLP. La
  [entrada sobre las métricas de OpenTelemetry](/blog/opentelemetry-metrics/) explica el
  pipeline.
- **Guardrails de entrada.** Políticas de escáneres con nombre que bloquean u observan las
  peticiones antes de que lleguen a un proveedor. Consulta
  [Política de guardrails](/docs/configuration/guardrails-policy/).

Synapse se distribuye con licencia MPL-2.0, así que puedes integrarlo en productos comerciales, y nada de esto está detrás de un nivel de pago.

## Ejecútalo o embébelo {#run-it-or-embed-it}

Puedes ejecutar Synapse como un único binario o como la imagen de Docker
`sustentabilitas/synapse-gateway`, con la API en el puerto 8080 y las métricas en el 9090. La
[guía de inicio rápido](/docs/get-started/quickstart/) te deja un gateway respondiendo en pocos
minutos.

Si tu servicio está escrito en Rust, puedes prescindir del proceso adicional por completo.
Añade como dependencia el crate `synapse-gateway` con `default-features = false`, construye un
`Gateway` en código y llama a `Gateway::chat()` en el mismo proceso, con el mismo comportamiento
de enrutamiento, fallback y registro de costes que el binario y sin ningún salto HTTP. Consulta
[Embeber Synapse como biblioteca](/docs/guides/embedding-as-library/).

## La familia {#the-family}

Synapse es un workspace de Cargo, y el gateway tiene hermanos que surgieron de las mismas
necesidades:

- **[`synapse-proxy`](/docs/synapse-family/proxy/overview/)** es un sidecar de proxy inverso
  guiado por configuración. Enruta por prefijo de path y estampa una identidad fijada, como un
  tenant, en cada petición reenviada, para que una carga de trabajo aislada no pueda elegir la
  suya.
- **[`synapse-a2a`](/docs/synapse-family/a2a/overview/)** es un registro de agentes
  agent-to-agent (A2A), con registro administrativo y descubrimiento público, servido por el
  binario del gateway.
- **[`synapse-mcp`](/docs/synapse-family/mcp/overview/)** es una biblioteca de gateway MCP bajo
  demanda que enruta las llamadas a herramientas de Streamable HTTP por servidor e inyecta la
  identidad del tenant actual.
- **`synapse-context`** es el almacén de contexto compartido que hay detrás del proxy y del
  gateway MCP.

[Crates del workspace](/docs/internals/workspace-crates/) muestra cómo dependen unos de otros.

## Qué viene ahora {#whats-next}

Preferimos que conozcas las carencias aquí y no en producción. La autenticación de entrada, la
limitación de tasa y la recarga dinámica de rutas están planificadas; hoy, ejecuta Synapse
detrás de tu propio API gateway, ingress o service mesh. El gateway registra métricas pero no
emite spans de trazas, y los tramos de chat tienen un solo intento cada uno, sin reintentos ni
circuit breakers. La página de [limitaciones y hoja de ruta](/docs/reference/limitations-roadmap/)
enumera cada una de ellas, con enlaces a los detalles.

El código está en [GitHub](https://github.com/sustentabilitas/synapse-gateway), y las
[contribuciones](/docs/contributing/) son bienvenidas. Prueba la guía de inicio rápido y
cuéntanos qué se rompe.
