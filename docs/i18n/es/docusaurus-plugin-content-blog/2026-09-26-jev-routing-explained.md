---
slug: jev-routing-explained
title: "El enrutamiento con Jev, explicado: el modelo y el esfuerzo adecuados para cada petición"
authors: [rajwilkhu]
tags: [routing]
date: 2026-09-28T09:00
description: Cómo una ruta strategy = "jev" de Synapse pregunta a TypeSafe Jev lo exigente que es cada petición, elige un nivel y un esfuerzo de razonamiento, informa de la decisión y se degrada de forma segura.
---

La mayoría de las aplicaciones envían todas las peticiones de una ruta al mismo modelo. Un
asistente de chat que responde a un «¡gracias!» y depura una condición de carrera en la misma
tarde paga por su modelo más potente en ambos casos, o ahorra en ambos y decepciona en el
difícil.

El router Jev de Synapse deja que decida la petición. Una ruta `strategy = "jev"` declara
niveles de dificultad y, para cada petición, el gateway pregunta a TypeSafe Jev lo exigente que
es y después la sirve desde el nivel correspondiente con el esfuerzo de razonamiento adecuado.
Esta entrada explica cómo se toma esa decisión, cuánto cuesta y qué pasa cuando no se puede
tomar.

<!-- truncate -->

## Los niveles describen trabajo, no modelos {#tiers-describe-work-not-models}

Una ruta Jev sustituye la lista única de tramos de una ruta por entre dos y diez niveles,
ordenados del más fácil al más difícil. Cada nivel tiene un nombre, una descripción del trabajo
para el que sirve, un esfuerzo de razonamiento y sus propios tramos:

```toml
[routes."auto"]
strategy = "jev"

[routes."auto".jev_router]
default_tier = "moderate"

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

Una ruta `jev` necesita `TYPESAFE_API_KEY`; sin ella, la validación estricta de proveedores que
se aplica por defecto impide que el gateway arranque.

Las descripciones importan más que cualquier otra cosa del archivo, porque son aquello con lo
que Jev compara la petición. `"Multi-step analysis, maths or proofs"` (análisis en varios
pasos, matemáticas o demostraciones) le da a Jev algo que juzgar; `"Gemini Pro"`, no. El orden también importa: es lo que indexa la
puntuación de dificultad y determina el fallback entre niveles. El
[tutorial de niveles de Jev](/docs/get-started/tutorials/jev-tiers/) construye esta ruta paso a
paso.

## Dos preguntas por petición {#two-questions-per-request}

Antes de servir una petición, Synapse envía a Jev un resumen acotado de la conversación: el
último mensaje del usuario, el prompt de sistema y tanto historial reciente como quepa, unos
24,000 caracteres en total, además de si la petición lleva herramientas o medios. Jev responde a
dos preguntas tipadas sobre ella.

**Dificultad** es una puntuación frente a las descripciones de tus niveles, en orden. Synapse la
redondea al nivel más cercano. Jev también informa de su confianza en esa puntuación; por debajo
de `min_confidence` (0.5 por defecto), el gateway ignora la puntuación y sirve la petición desde
`default_tier`.

**Necesita razonamiento** es la probabilidad de que una buena respuesta requiera un razonamiento
cuidadoso paso a paso, como en matemáticas, lógica, planificación o depuración. Si alcanza o
supera `reasoning_threshold` (0.7 por defecto), el esfuerzo del nivel sube un escalón, incluso
cuando la puntuación de dificultad era demasiado incierta para usarla.

La llamada de decisión está acotada por `timeout_ms`, 400 ms por defecto, así que puede añadir
hasta ese tiempo de latencia a cada petición de la ruta. Ese es el precio de la decisión, y la razón por la que
una ruta estática sigue siendo mejor opción cuando ya sabes qué modelo necesita cada caso de
uso.

## El esfuerzo, traducido para cada proveedor {#effort-translated-per-provider}

El `effort` de cada nivel es uno de `none`, `minimal`, `low`, `medium`, `high`, `xhigh` o `max`,
y el aumento por razonamiento avanza un escalón en esa lista, sin pasar de `max`. Synapse
traduce el esfuerzo para cada tramo:

- En el carril estándar, los proveedores de estilo OpenAI reciben `reasoning_effort`, con `max`
  enviado como `xhigh`, y los modelos Gemini 3 de Vertex reciben un `thinkingLevel`.
- En el carril Vertex nativo, Synapse fija él mismo el presupuesto de razonamiento
  (`thinkingBudget`): 512 tokens para `minimal`, 1,024 para `low`, 4,096 para `medium`, 8,192
  para `high`, 16,384 para `xhigh` y 24,576 para `max`.
- `none` no envía nada, así que se aplica el valor por defecto del propio modelo. En algunos
  modelos Gemini ese valor por defecto es el razonamiento dinámico, así que usa `minimal` cuando
  quieras que el razonamiento se mantenga reducido.

La tabla de [Esfuerzo](/docs/configuration/routes/#effort) de la referencia de rutas tiene todas
las correspondencias. Los tokens de razonamiento se facturan como salida, por eso los niveles fáciles
suelen ir con `none` o `minimal` y solo los difíciles reciben `medium` o más.

El esfuerzo que indica el propio cliente siempre gana en su carril. Un `reasoning_effort` en el
cuerpo de la petición sustituye el esfuerzo del nivel en el carril estándar, y un
`vertex.thinking_config` sustituye el `thinkingBudget` del nivel en el carril nativo. Jev sigue
eligiendo el nivel, y la respuesta indica `x-synapse-reasoning-effort: client`.

## Cada respuesta indica qué ha ocurrido {#every-response-says-what-happened}

Cada chat completion lleva `x-synapse-routing`: `jev` en una ruta Jev, `static-override` cuando
el cliente envió `"routing_strategy": "static"` para saltarse la decisión, y `static` en las
rutas normales. Cuando corresponde, `x-synapse-tier` nombra el nivel que sirvió la petición,
`x-synapse-tier-decided` el nivel que eligió la decisión (la elección de Jev, o `default_tier`
cuando el enrutamiento está degradado) si sirvió otro distinto, y `x-synapse-reasoning-effort`
el esfuerzo con el que se ejecutó el tramo que la sirvió. En los streams, los encabezados
describen el tramo que produjo el primer fragmento. La sección de
[encabezados de respuesta](/docs/guides/jev-router/#response-headers) enumera todos los valores.

## Un problema con Jev nunca hace fallar la petición {#a-jev-problem-never-fails-the-request}

El router está diseñado para degradarse, no para romperse. Si Jev agota el tiempo de espera,
devuelve un error, responde con poca confianza o el gateway no tiene ningún cliente de Jev, la
petición va a `default_tier` y la respuesta lleva `x-synapse-routing-degraded` con el motivo:
`timeout`, `error`, `low_confidence` o `jev_unavailable`. Elige un `default_tier` que atienda de
forma aceptable la mayoría de las peticiones, normalmente uno intermedio.

Si fallan los tramos del nivel que sirve la petición, Synapse prueba cada nivel más difícil,
empezando por el más cercano, y después cada nivel más fácil. Las peticiones con funcionalidades
nativas de Vertex solo usan tramos `vertex`, así que van al nivel más cercano que tenga uno. La
petición solo falla cuando fallan todos los tramos elegibles, exactamente igual que en una ruta
estática. [Comportamiento ante fallos](/docs/guides/jev-router/#failure-behaviour) tiene los
detalles.

## Decisiones que puedes auditar {#decisions-you-can-audit}

Cada decisión que responde Jev escribe su propia fila en el
[registro de costes](/docs/guides/cost-ledger/), con proveedor `typesafe`, carril `jev` y el
mismo `request_id` que la petición de chat que enrutó, valorada como `typesafe:<model>` según
tu `pricing.toml`. Ves el coste de decidir junto al coste de responder.

Dos métricas siguen al router: `synapse_routing_decisions_total`, etiquetada por ruta, nivel y
resultado, y `synapse_routing_decision_duration_seconds`. Si crece la proporción de resultados
`timeout` o `error`, las peticiones están acabando en `default_tier` sin una decisión real, así
que sube `timeout_ms` o comprueba la disponibilidad de Jev.
[Métricas](/docs/operating/metrics/#queries) tiene una consulta lista para usar con esa
proporción.

## Pruébalo {#try-it}

Empieza con el [tutorial de niveles de Jev](/docs/get-started/tutorials/jev-tiers/) y ten a
mano la [guía del router Jev](/docs/guides/jev-router/) mientras ajustas las descripciones de
los niveles y los umbrales con tu propio tráfico.
