---
sidebar_position: 2
title: Enrutar peticiones por dificultad con Jev
description: Crea una ruta strategy = "jev" con tres niveles, envía un prompt fácil y otro difícil, y comprueba qué nivel y qué esfuerzo de razonamiento eligió Synapse para cada uno.
---

En este tutorial añades una ruta `auto` que elige un modelo y un esfuerzo de razonamiento
para cada petición. En lugar de una lista fija de tramos, la ruta declara tres niveles de
dificultad. Antes de servir una petición, Synapse pregunta a TypeSafe Jev lo exigente que
es y si necesita razonamiento paso a paso, y después la sirve desde el nivel
correspondiente. Envías un prompt fácil y otro difícil y lees la decisión en los
encabezados de respuesta, el registro de costes y las métricas.

## Antes de empezar {#before-you-begin}

- Completa el [Inicio rápido](../quickstart.md). Este tutorial reutiliza su directorio
  `synapse-quickstart` y sus credenciales.
- Consigue una clave de API de TypeSafe. Jev toma la decisión de enrutamiento, así que una ruta
  `jev` necesita `TYPESAFE_API_KEY`.

## Añadir una ruta con niveles {#add-a-tiered-route}

Sustituye `config/routes.toml` por este archivo. Mantiene las dos rutas del inicio rápido y
añade `auto`:

```toml title="config/routes.toml"
[routes."gemini-flash"]
legs = [
  { provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" },
]

[routes."chat"]
legs = [
  { provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" },
  { provider = "openai", model = "gpt-4o-mini" },
]

[routes."auto"]
strategy = "jev"

[routes."auto".jev_router]
default_tier = "moderate"
timeout_ms = 400

[[routes."auto".tiers]]
name = "trivial"
description = "Greetings, chit-chat, one-line lookups or rewrites"
effort = "none"
legs = [{ provider = "vertex", model = "gemini-3.5-flash-lite", region = "us" }]

[[routes."auto".tiers]]
name = "moderate"
description = "Everyday Q&A, summarising, simple extraction or code edits"
effort = "low"
legs = [{ provider = "vertex", model = "gemini-3.6-flash", region = "global" }]

[[routes."auto".tiers]]
name = "hard"
description = "Multi-step analysis, maths or proofs, non-trivial code or debugging"
effort = "medium"
legs = [{ provider = "vertex", model = "gemini-3.1-pro-preview", region = "global" }]
```

Lo que declara la ruta `auto`:

- `strategy = "jev"` sustituye `legs` por niveles, listados del más fácil al más difícil.
  Una ruta tiene de 2 a 10 niveles.
- Jev puntúa cada petición frente a las `description` de los niveles, así que estas
  describen el trabajo, nunca el modelo. Los `name` de los niveles se devuelven en los
  encabezados de respuesta, así que deben ser únicos y en ASCII imprimible.
- `effort` es el esfuerzo de razonamiento con el que se ejecutan los tramos del nivel:
  `none`, `minimal`, `low`, `medium`, `high`, `xhigh` o `max`. En el carril estándar,
  Synapse lo pasa al crate `genai`, que lo envía como `reasoning_effort` a los proveedores
  de estilo OpenAI y como `thinkingLevel` a estos modelos Gemini 3. `none` no envía nada,
  así que se aplica el valor por defecto del modelo. Consulta
  [Esfuerzo](../../configuration/routes.md#effort) para ver la correspondencia completa.
- `default_tier` sirve cuando Jev no está seguro, tarda demasiado o no está disponible.
  `timeout_ms` limita la llamada de decisión; 400 ms es el valor por defecto.
- Los tramos de un nivel pueden usar cualquier proveedor excepto `typesafe`: Jev decide, no
  es un candidato.

[Rutas Jev](../../configuration/routes.md#jev-routes), en la referencia de rutas, enumera
cada clave y su valor por defecto.

Añade a `config/pricing.toml` los precios de los nuevos modelos y de las decisiones de Jev,
para que el registro pueda calcular su coste (consulta
[Precios](../../configuration/pricing.md)):

```toml title="config/pricing.toml"
"vertex:gemini-3.5-flash-lite" = { input = 0.30, output = 2.50 }
"vertex:gemini-3.6-flash" = { input = 1.50, output = 7.50 }
"vertex:gemini-3.1-pro-preview" = { input = 2.00, output = 12.00 }
"openai:gpt-4o-mini" = { input = 0.15, output = 0.60 }
"typesafe:jev-latest" = { input = 0.042, output = 0.0 }
```

## Reiniciar el gateway con una clave de TypeSafe {#restart-the-gateway-with-a-typesafe-key}

Synapse lee su configuración al arrancar, así que detén el gateway y vuelve a arrancarlo
con `TYPESAFE_API_KEY`. Con Docker:

```bash
docker run --rm -p 8080:8080 -p 9090:9090 \
  -e VERTEX_PROJECT_ID=my-gcp-project \
  -e OPENAI_API_KEY=sk-... \
  -e TYPESAFE_API_KEY=... \
  -e GOOGLE_APPLICATION_CREDENTIALS=/secrets/sa.json \
  -e SYNAPSE_LEDGER_SQLITE_DSN="sqlite:///app/data/synapse.db?mode=rwc" \
  -v "$(pwd)/sa.json:/secrets/sa.json:ro" \
  -v "$(pwd)/config:/app/config" \
  -v "$(pwd)/data:/app/data" \
  sustentabilitas/synapse-gateway
```

Con Cargo, ejecuta `export TYPESAFE_API_KEY=...` y vuelve a lanzar el comando
`synapse-gateway` del inicio rápido.

Una ruta `jev` hace referencia al proveedor `typesafe`, así que con la validación de
proveedores `strict` por defecto el gateway se niega a arrancar sin `TYPESAFE_API_KEY`. Con
`SYNAPSE_PROVIDER_VALIDATION=lenient` arranca, pero `auto` pasa a ser una ruta estática que
siempre prueba primero `moderate`, después `hard` y después `trivial`.

## Enviar un prompt fácil {#send-an-easy-prompt}

Envía un saludo a `auto` y muestra solo los encabezados de enrutamiento de Synapse:

```bash
curl -s -D - -o /dev/null http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "auto",
    "messages": [{"role": "user", "content": "Hi there!"}]
  }' | grep -i '^x-synapse'
```

Jev califica un saludo como trivial, así que normalmente verás:

```text
x-synapse-routing: jev
x-synapse-tier: trivial
x-synapse-reasoning-effort: none
```

- `x-synapse-routing: jev` indica que Jev tomó la decisión.
- `x-synapse-tier` es el nivel que sirvió la petición.
- `x-synapse-reasoning-effort` es el esfuerzo con el que se ejecutó el tramo de ese nivel.

Quita `-D - -o /dev/null` y el `grep` para ver el cuerpo de la respuesta: un
`chat.completion` normal cuyo `model` es `gemini-3.5-flash-lite`.

## Enviar un prompt difícil {#send-a-hard-prompt}

Ahora pide una demostración:

```bash
curl -s -D - -o /dev/null http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -d '{
    "model": "auto",
    "messages": [{"role": "user", "content": "Prove that there are infinitely many primes of the form 4k + 3."}]
  }' | grep -i '^x-synapse'
```

Encabezados habituales:

```text
x-synapse-routing: jev
x-synapse-tier: hard
x-synapse-reasoning-effort: high
```

La petición fue al nivel `hard`. Su esfuerzo configurado es `medium`, pero Jev también
juzgó que el prompt necesita razonamiento paso a paso, así que Synapse subió el esfuerzo un
paso, a `high`. Lo sube cuando la puntuación de razonamiento de Jev alcanza
`reasoning_threshold` (por defecto, 0.7).

Las calificaciones de Jev dependen del prompt, así que tus niveles pueden ser distintos.
Prueba tus propios prompts y observa cómo cambian los encabezados.

## Anular la decisión {#override-the-decision}

Un cliente puede fijar su propio esfuerzo. Añade `"reasoning_effort": "low"` al cuerpo de
la petición: Jev sigue eligiendo el nivel, pero el tramo usa tu esfuerzo y el encabezado
muestra `x-synapse-reasoning-effort: client`.

Un cliente también puede omitir la decisión. Añade `"routing_strategy": "static"` y Synapse
no llama a Jev: la petición va a `default_tier` con el esfuerzo de ese nivel, y los
encabezados muestran:

```text
x-synapse-routing: static-override
x-synapse-tier: moderate
x-synapse-reasoning-effort: low
```

En una ruta sin niveles, `"routing_strategy": "jev"` devuelve `400`.

## Ver las decisiones en el registro {#see-the-decisions-in-the-ledger}

Cada decisión que toma Jev se escribe en el registro como una fila propia, que comparte el
`request_id` de la petición de chat que enrutó:

```bash
sqlite3 data/synapse.db \
  "SELECT request_id, provider, model, lane, input_tokens, cost_usd
   FROM usage_events WHERE route = 'auto' ORDER BY id;"
```

Por cada petición enrutada por Jev hay dos filas con el mismo `request_id`: la decisión,
con el proveedor `typesafe`, el modelo `jev-latest` y el carril `jev`, y el chat
completion, con el modelo de Vertex del nivel que la sirvió y el carril `standard`. La
petición `static-override` solo tiene la fila del chat, porque no se llamó a Jev.

## Ver las decisiones en las métricas {#see-the-decisions-in-the-metrics}

```bash
curl -s http://localhost:9090/metrics | grep synapse_routing
```

`synapse_routing_decisions_total` cuenta las decisiones por `route`, el `tier` decidido y
el `outcome` (`decided`, `static_override`, `low_confidence`, `timeout` o `error`).
`synapse_routing_decision_duration_seconds` registra cuánto tardó cada llamada a Jev.

## Cuando Jev no puede decidir {#when-jev-cannot-decide}

Un problema con Jev nunca hace fallar la petición. Si Jev agota el tiempo de espera,
devuelve un error o responde con una confianza inferior a `min_confidence` (por defecto,
0.5), la petición va a `default_tier` y la respuesta lleva `x-synapse-routing-degraded`
con el motivo: `timeout`, `error`, `low_confidence` o `jev_unavailable`.

Si fallan los tramos del nivel elegido, Synapse prueba primero los niveles más difíciles y
después los más fáciles. Cuando acaba sirviendo otro nivel, `x-synapse-tier-decided` indica
el nivel que eligió Jev.
[Comportamiento ante fallos](../../guides/jev-router.md#failure-behaviour), en la guía del
router Jev, explica cada motivo y cómo supervisarlo.

## Próximos pasos {#next-steps}

- [Haz fallback entre proveedores](fallback-across-providers.md) para ver cómo se recupera
  una ruta cuando falla un tramo.
- Consulta [niveles y estrategias](../../overview/concepts.md#tier) en Conceptos.
- Ajusta la decisión con las [claves de `jev_router`](../../configuration/routes.md#jev-routes).
- Escribe mejores niveles y lee sobre el comportamiento ante fallos en la
  [guía del router Jev](../../guides/jev-router.md).
