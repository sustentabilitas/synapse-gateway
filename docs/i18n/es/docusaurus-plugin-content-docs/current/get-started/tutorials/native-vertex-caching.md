---
sidebar_position: 1
title: Guardar un vídeo en caché y obtener respuestas estructuradas
description: Crea una caché de contexto de Vertex AI, consúltala a través del carril Vertex nativo de Synapse con un esquema de respuesta y encuentra las peticiones en el registro de costes y en las métricas.
---

En este tutorial guardas un vídeo en la caché de Vertex AI una sola vez, haces preguntas
sobre él a Gemini a través de Synapse sin volver a enviar el vídeo y restringes las
respuestas a un esquema JSON. Ambas funcionalidades solo existen en Vertex AI, así que
Synapse sirve estas peticiones en su carril Vertex nativo. Al final, buscas las peticiones
en el registro de costes y en las métricas.

## Antes de empezar {#before-you-begin}

- Completa el [Inicio rápido](../quickstart.md) y mantén el gateway en marcha con su ruta
  `gemini-flash`. Este tutorial se ejecuta desde el mismo directorio `synapse-quickstart`.
- Instala la [CLI de Google Cloud](https://cloud.google.com/sdk/docs/install) e inicia
  sesión con una cuenta que pueda usar Vertex AI en tu proyecto (`gcloud auth login`).

Los ejemplos usan el proyecto `my-gcp-project`; sustitúyelo por el tuyo.

## Crear una caché de contexto {#create-a-context-cache}

Una caché de contexto almacena contenido, aquí un vídeo de ejemplo público de 98 segundos,
en Vertex AI para que las peticiones posteriores puedan referirse a él por su nombre en
lugar de volver a enviarlo. Vertex AI factura los tokens en caché con descuento y cobra el
almacenamiento hasta que la caché caduca. Consulta la guía de Google para
[crear una caché de contexto](https://cloud.google.com/vertex-ai/generative-ai/docs/context-cache/context-cache-create)
y ver todas las opciones.

La caché debe ser para el mismo modelo y la misma ubicación que el tramo de Vertex de la
ruta. La ruta `gemini-flash` usa `gemini-3.5-flash-lite` en la multirregión `us`, así que
crea la caché allí:

```bash
curl -s -X POST \
  -H "Authorization: Bearer $(gcloud auth print-access-token)" \
  -H "Content-Type: application/json" \
  https://aiplatform.us.rep.googleapis.com/v1/projects/my-gcp-project/locations/us/cachedContents \
  -d '{
    "model": "projects/my-gcp-project/locations/us/publishers/google/models/gemini-3.5-flash-lite",
    "displayName": "synapse-tutorial-animals",
    "contents": [{
      "role": "user",
      "parts": [{
        "fileData": {
          "mimeType": "video/mp4",
          "fileUri": "gs://cloud-samples-data/video/animals.mp4"
        }
      }]
    }],
    "ttl": "3600s"
  }'
```

La respuesta describe la nueva caché:

```json
{
  "name": "projects/123456789012/locations/us/cachedContents/1234567890123456789",
  "model": "projects/my-gcp-project/locations/us/publishers/google/models/gemini-3.5-flash-lite",
  "createTime": "2026-09-28T10:00:00.000000Z",
  "updateTime": "2026-09-28T10:00:00.000000Z",
  "expireTime": "2026-09-28T11:00:00.000000Z"
}
```

Copia el valor de `name` en una variable de shell. La caché caduca tras el `ttl` de una
hora:

```bash
export CACHE="projects/123456789012/locations/us/cachedContents/1234567890123456789"
```

:::note
Vertex AI solo guarda en caché contenido que supere un número mínimo de tokens, que depende
del modelo. Si guardas en caché tu propio contenido y la llamada de creación falla,
consulta los límites en la
[visión general de la caché de contexto](https://cloud.google.com/vertex-ai/generative-ai/docs/context-cache/context-cache-overview)
de Google.
:::

## Preguntar sobre el vídeo en caché {#ask-a-question-about-the-cached-video}

Envía un chat completion a `gemini-flash` con el nombre de la caché en el bloque `vertex`.
El vídeo no va en la petición; Gemini lo lee de la caché:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d @- <<EOF
{
  "model": "gemini-flash",
  "messages": [{"role": "user", "content": "Which animals appear in the video?"}],
  "vertex": { "cached_content": "$CACHE" }
}
EOF
```

La respuesta es un `chat.completion` estándar cuyo mensaje enumera los animales. Como la
petición tiene un bloque `vertex` con `cached_content`, Synapse la envía al carril Vertex
nativo, que pasa el nombre a Vertex AI como `cachedContent`. El carril estándar tendría que
descartar ese campo.

Si la caché no existe, ha caducado o se creó para otro modelo u otra ubicación, Vertex AI
rechaza la petición y Synapse devuelve `400` con el mensaje de error de Vertex.

## Obtener una respuesta estructurada {#get-a-structured-answer}

Añade un `response_schema` al bloque `vertex` para que Gemini responda con JSON que cumpla
el esquema:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d @- <<EOF
{
  "model": "gemini-flash",
  "messages": [{"role": "user", "content": "List the animals in the video and describe the setting."}],
  "vertex": {
    "cached_content": "$CACHE",
    "response_schema": {
      "type": "object",
      "properties": {
        "animals": { "type": "array", "items": { "type": "string" } },
        "setting": { "type": "string" }
      },
      "required": ["animals", "setting"]
    }
  }
}
EOF
```

Synapse envía el esquema como `generationConfig.responseSchema` con `responseMimeType`
establecido en `application/json`, de modo que Vertex AI restringe la decodificación al
esquema. El contenido del mensaje es una cadena JSON que puedes parsear directamente:

```json
{"animals": ["..."], "setting": "..."}
```

Synapse deja fuera del contenido las partes de razonamiento de Gemini, así que la cadena es
únicamente el documento JSON.

## Ver las peticiones en el registro {#see-the-requests-in-the-ledger}

Consulta el registro de SQLite para ver las dos peticiones del carril nativo que acabas de
enviar:

```bash
sqlite3 data/synapse.db \
  "SELECT route, lane, model, input_tokens, output_tokens, cost_usd
   FROM usage_events WHERE lane = 'native' ORDER BY id DESC LIMIT 2;"
```

Ambas filas muestran la ruta `gemini-flash`, el carril `native` y el modelo
`gemini-3.5-flash-lite`, atribuidas a `my-team`. `input_tokens` es el `promptTokenCount` de
Vertex AI, que incluye los tokens del vídeo en caché, así que es mucho mayor que tu breve
prompt.

:::warning
Synapse valora cada token de entrada al precio `input` de `pricing.toml`, esté en caché o
no. Vertex AI factura los tokens en caché con descuento (90% en Gemini 2.5 y posteriores) y
cobra aparte el almacenamiento de la caché, así que el registro sobrestima el coste de las
peticiones que usan caché.
:::

## Ver las peticiones en las métricas {#see-the-requests-in-the-metrics}

El endpoint de métricas en el puerto `9090` cuenta las peticiones por ruta, modelo, sistema
del proveedor y carril:

```bash
curl -s http://localhost:9090/metrics | grep 'lane="native"'
```

Busca las series `synapse_requests_total` y `synapse_input_tokens_total` con las etiquetas
`route="gemini-flash"`, `lane="native"` y `system="vertexai"`.

## Limpiar {#clean-up}

Elimina la caché para dejar de pagar su almacenamiento antes de que caduque:

```bash
curl -s -X DELETE \
  -H "Authorization: Bearer $(gcloud auth print-access-token)" \
  "https://aiplatform.us.rep.googleapis.com/v1/$CACHE"
```

## Próximos pasos {#next-steps}

- [Enruta peticiones por dificultad con Jev](jev-tiers.md) para elegir un modelo y un
  esfuerzo de razonamiento en cada petición.
- Lee sobre el [carril Vertex nativo](../../overview/architecture.md#native-vertex-lane),
  incluidos `media_uris` y `thinking_config`.
- Usa todas las funcionalidades de Vertex nativo, desde la salida estructurada hasta las
  llamadas a herramientas, con la [guía de Vertex nativo](../../guides/native-vertex.md).
