---
sidebar_position: 3
title: Hacer fallback entre proveedores
description: Rompe a propósito el primer tramo de una ruta con dos proveedores, observa cómo OpenAI sirve la petición en su lugar y aprende en qué se diferencia el fallback para clientes con y sin streaming.
---

En este tutorial haces fallar a propósito el primer tramo de la ruta `chat` del inicio
rápido y observas cómo responde en su lugar su segundo tramo, OpenAI. Por el camino ves qué
recibe un cliente cuando fallan todos los tramos y cómo funciona el fallback en las
peticiones con y sin streaming.

## Antes de empezar {#before-you-begin}

Completa el [Inicio rápido](../quickstart.md), incluida una `OPENAI_API_KEY`: este tutorial
necesita los dos tramos de la ruta `chat`. Se ejecuta desde el mismo directorio
`synapse-quickstart`.

## Comprobar la ruta en buen estado {#check-the-healthy-route}

Con la configuración del inicio rápido, envía una petición a `chat`:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "chat",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

El campo `model` de la respuesta indica el tramo que ha respondido:
`gemini-3.5-flash-lite`, el primer tramo de la ruta.

## Romper el primer tramo {#break-the-first-leg}

Sustituye `config/routes.toml` por este archivo, que hace que el primer tramo de `chat`
apunte a un modelo que no existe:

```toml title="config/routes.toml"
[routes."gemini-flash"]
legs = [
  { provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" },
]

[routes."chat"]
legs = [
  { provider = "vertex", model = "gemini-does-not-exist", region = "us" },
  { provider = "openai", model = "gpt-4o-mini" },
]
```

Synapse lee su configuración al arrancar, así que detén el gateway y vuelve a arrancarlo
con el mismo comando que en el inicio rápido.

## Ver cómo sirve el segundo tramo {#watch-the-second-leg-serve}

Envía de nuevo la misma petición:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "chat",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

Esta vez la respuesta llega con `"model": "gpt-4o-mini"`. Vertex AI rechazó el primer
tramo, así que Synapse pasó al siguiente tramo de la ruta y devolvió la respuesta de
OpenAI. El cliente envió la misma petición y recibió una única respuesta correcta; nunca
vio el intento fallido.

El registro guarda el tramo que sirvió la petición:

```bash
sqlite3 data/synapse.db \
  "SELECT route, provider, model, cost_usd FROM usage_events ORDER BY id DESC LIMIT 1;"
```

La última fila muestra la ruta `chat`, el proveedor `openai` y el modelo `gpt-4o-mini`,
valorada con la entrada `openai:gpt-4o-mini` de `pricing.toml`.

## Streaming con fallback {#stream-with-fallback}

Las peticiones en streaming también hacen fallback, siempre que no haya llegado nada al
cliente todavía:

```bash
curl -sN http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "chat",
    "messages": [{"role": "user", "content": "Count to 5."}],
    "stream": true
  }'
```

El primer tramo falla antes de producir ninguna salida, así que Synapse inicia el stream
desde el tramo de OpenAI, y cada `chat.completion.chunk` lleva `"model": "gpt-4o-mini"`.

## Cuando fallan todos los tramos {#when-every-leg-fails}

Reinicia el gateway con `OPENAI_API_KEY=sk-invalid` para que el segundo tramo también
falle, y vuelve a enviar la petición sin streaming. Synapse devuelve `502` con el código de
error `all_legs_failed` y una entrada por cada tramo fallido:

```json
{
  "error": {
    "type": "all_legs_failed",
    "code": "all_legs_failed",
    "message": "all legs of route 'chat' failed",
    "failures": [
      { "provider": "vertex", "model": "gemini-does-not-exist", "message": "..." },
      { "provider": "openai", "model": "gpt-4o-mini", "message": "..." }
    ]
  }
}
```

Cada `message` contiene el error del proveedor, que te dice por qué falló el tramo.

## Fallback con y sin streaming {#streaming-and-non-streaming-fallback}

Synapse siempre hace streaming desde el proveedor internamente, así que la diferencia entre
los dos tipos de cliente es lo que ya se ha enviado cuando falla un tramo:

- **Los clientes sin streaming** disponen de la cadena completa. Synapse consolida el
  stream de cada tramo en una única respuesta antes de enviar nada, así que cualquier fallo
  pasa al siguiente tramo: una respuesta de error, ningún primer fragmento dentro de
  `SYNAPSE_REQUEST_TIMEOUT_SECS` (por defecto, 120), una pausa más larga que
  `SYNAPSE_STREAM_IDLE_TIMEOUT_SECS` (por defecto, 60) o un stream que se corta a medias.
- **Los clientes con streaming** disponen de fallback hasta el primer fragmento. Synapse se
  compromete con el primer tramo que produce un fragmento y lo reenvía de inmediato. Si ese
  tramo falla más tarde, ya es demasiado tarde para cambiar: el stream termina con un
  evento `data: {"error": {"type": "upstream_error", ...}}` seguido de `data: [DONE]`.

Estas reglas se aplican al carril estándar, que sirvió todas las peticiones de este
tutorial. Una petición Vertex nativa usa solo los tramos `vertex` de la ruta, así que en
`chat` el tramo de OpenAI no puede rescatarla. Entre tramos `vertex`, el carril nativo solo
pasa al siguiente tras una respuesta `5xx`, `429` o `408`, un error de conexión o un tiempo
de espera agotado; consulta
[Flujo de una petición](../../overview/architecture.md#request-flow).

## Restaurar la ruta {#restore-the-route}

Devuelve el primer tramo de `chat` a `gemini-3.5-flash-lite`, restaura tu
`OPENAI_API_KEY` real y reinicia el gateway.

## Próximos pasos {#next-steps}

- Consulta las [reglas de fallback](../../configuration/routes.md#fallback) en la
  referencia de rutas.
- Diseña tus propias cadenas con la
  [guía de cadenas de fallback](../../guides/fallback-chains.md), que cubre todos los
  carriles y lo que se registra.
- Lee [Arquitectura](../../overview/architecture.md) para ver cómo encaja el fallback en el
  flujo de una petición.
- Consulta [cadenas de fallback](../../overview/concepts.md#fallback-chain) en Conceptos.
