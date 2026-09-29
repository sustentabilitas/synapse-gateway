---
sidebar_position: 4
title: Streaming y llamadas a herramientas
description: Cómo hace streaming Synapse de las respuestas, cómo interactúa el streaming con el fallback y los tiempos de espera, y cómo funcionan las llamadas a herramientas en los carriles estándar y Vertex nativo.
---

Usa streaming cuando una persona espera la salida, para que el texto aparezca a medida que el
modelo lo escribe; usa peticiones sin streaming para trabajo en segundo plano, donde la cadena
de fallback completa importa más que el primer token. Usa llamadas a herramientas cuando el
modelo necesite llamar a tus funciones. Ambas cosas funcionan con el formato de petición
estándar de OpenAI, en los carriles estándar y Vertex nativo, así que el código existente con
el SDK de OpenAI funciona sin cambios.

## Streaming {#streaming}

Establece `"stream": true` para recibir server-sent events: una secuencia de objetos
`chat.completion.chunk`, cada uno en una línea `data: `, que termina con `data: [DONE]`. El
último fragmento antes de `[DONE]` tiene un `delta` vacío y el `finish_reason`. Sin `stream`,
recibes un único objeto `chat.completion`.

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

Los fragmentos del stream no llevan objeto `usage`. Synapse cuenta igualmente los tokens:
están en el [registro de costes](cost-ledger.md) y en las métricas
`synapse_input_tokens_total` y `synapse_output_tokens_total`.

Synapse siempre hace streaming desde el proveedor, pida lo que pida el cliente. Para un
cliente sin streaming, consolida el stream en una única respuesta antes de enviar nada,
y por eso las peticiones sin streaming conservan la cadena de fallback completa.

### Fallback durante el streaming {#fallback-while-streaming}

Una petición con streaming solo puede hacer fallback hasta que su primer fragmento llega al
cliente. Synapse se compromete con el primer tramo que produce un fragmento; si ese tramo
falla después, ya es tarde para cambiar, porque el cliente ya tiene parte de la respuesta. El
estado HTTP ya es `200`, así que el fallo llega como un evento:

```text
data: {"error":{"type":"upstream_error","code":"upstream_error","message":"..."}}

data: [DONE]
```

Los clientes deberían tratar un evento `error` como una respuesta fallida y descartar el texto
parcial. Las peticiones sin streaming en el carril estándar nunca lo ven: un tramo que se corta
a mitad de camino se abandona y el siguiente tramo empieza desde cero.
[Cadenas de fallback](fallback-chains.md) describe qué hace avanzar la cadena en cada carril.

## Tiempos de espera {#timeouts}

Dos ajustes limitan cuánto espera Synapse a un proveedor, y se aplican de forma distinta según
el carril y el tipo de cliente:

| Petición | Primer fragmento | Intervalo entre fragmentos | Respuesta completa |
|---|---|---|---|
| Carril estándar, sin streaming | `SYNAPSE_REQUEST_TIMEOUT_SECS`; se prueba el siguiente tramo | `SYNAPSE_STREAM_IDLE_TIMEOUT_SECS`; se prueba el siguiente tramo | `SYNAPSE_REQUEST_TIMEOUT_SECS` |
| Carril estándar, con streaming | `SYNAPSE_REQUEST_TIMEOUT_SECS`; se prueba el siguiente tramo | No se aplica | `SYNAPSE_REQUEST_TIMEOUT_SECS` |
| Carril Vertex nativo | No se aplica | No se aplica | `SYNAPSE_REQUEST_TIMEOUT_SECS` |

`SYNAPSE_REQUEST_TIMEOUT_SECS` (por defecto 120) es también el tiempo de espera HTTP de cada
llamada a un proveedor, y cubre la respuesta completa, no solo la espera del primer byte. Una
respuesta que tarda más que eso en terminar falla, aunque sigan llegando tokens: antes del
primer fragmento hace fallback como cualquier otro fallo; después, el stream termina con un
evento `error`. Si esperas generaciones largas, como salidas estructuradas grandes o mucho
razonamiento, auméntalo. Consulta
[Variables de entorno](../configuration/environment-variables.md#streaming-and-timeouts).

## Llamadas a herramientas {#tool-calling}

Envía `tools` al estilo de OpenAI y Synapse las traduce para el carril que sirve la petición.
Cuando el modelo llama a una herramienta, el mensaje del asistente tiene `content: null` y un
array `tool_calls`, y `finish_reason` es `tool_calls`:

```json
{
  "model": "chat",
  "messages": [{ "role": "user", "content": "What's the weather in Lisbon?" }],
  "tools": [{
    "type": "function",
    "function": {
      "name": "get_weather",
      "description": "Current weather for a city",
      "parameters": {
        "type": "object",
        "properties": { "city": { "type": "string" } },
        "required": ["city"]
      }
    }
  }]
}
```

Ejecuta la función y vuelve a enviar la conversación con la llamada a herramienta del
asistente y un mensaje `role: "tool"` con el resultado:

```json
{
  "model": "chat",
  "messages": [
    { "role": "user", "content": "What's the weather in Lisbon?" },
    { "role": "assistant", "content": null, "tool_calls": [{
      "id": "call_0", "type": "function",
      "function": { "name": "get_weather", "arguments": "{\"city\":\"Lisbon\"}" }
    }] },
    { "role": "tool", "tool_call_id": "call_0", "name": "get_weather", "content": "21°C, sunny" }
  ],
  "tools": [ ... ]
}
```

En modo streaming, las llamadas a herramientas llegan como deltas `chat.completion.chunk` con
un `index` por llamada. El primer delta de una llamada lleva su `id` y `function.name`; los
deltas posteriores del mismo índice solo llevan fragmentos de `function.arguments` que hay que
concatenar.

Los dos carriles difieren en algunos detalles:

| | Carril estándar | Carril Vertex nativo |
|---|---|---|
| Definiciones de herramientas | Traducidas para el proveedor por el crate `genai`. | Enviadas como `functionDeclarations` de Vertex. |
| `tool_choice` | **No se reenvía.** El crate `genai` no tiene un campo para ello, así que decide el modelo. | Se respeta mediante `toolConfig.functionCallingConfig`. Consulta [Funcionalidades nativas de Vertex](native-vertex.md#tool-calling). |
| Ids de llamadas a herramientas | Los ids del proveedor. | Generados por Synapse: `call_0`, `call_1`, … |
| Argumentos en streaming | Fragmentos, tal como los envía el proveedor. | Los argumentos de cada llamada en un único delta. |
| Resultados de herramientas | Emparejados por `tool_call_id`. | Enviados a Vertex bajo el `name` del mensaje, así que establece `name` al nombre de la función. |

Si dependes de `tool_choice` para forzar o prohibir una llamada a herramienta en un modelo
Gemini, haz que la petición use el carril Vertex nativo añadiendo un bloque `vertex` con una
funcionalidad nativa, como `thinking_config`. Si no, aplica la elección en tu aplicación. El
carril estándar también descarta otros campos de la petición, incluido `max_tokens`; consulta
[Campos de la petición](../reference/limitations-roadmap.md#request-fields).
