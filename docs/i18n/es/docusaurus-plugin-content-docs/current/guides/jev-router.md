---
sidebar_position: 3
title: Router Jev
description: Cómo una ruta strategy = "jev" elige un nivel de modelo y un esfuerzo de razonamiento para cada petición, qué informa y qué ocurre cuando Jev no puede decidir.
---

Usa el router Jev cuando una ruta sirva peticiones de dificultad muy distinta, como un
asistente de chat que recibe tanto saludos como sesiones de depuración, y no quieras pagar por tu
modelo más potente en todas ellas. En lugar de una cadena fija de tramos, una ruta
`strategy = "jev"` declara niveles de dificultad. Para cada petición, Synapse pregunta a
TypeSafe Jev lo exigente que es y la sirve desde el nivel correspondiente con el esfuerzo de
razonamiento adecuado. Si ya sabes qué modelo necesita cada caso de uso, una ruta estática por
caso de uso es más sencilla y no añade latencia de decisión.

[Rutas Jev](../configuration/routes.md#jev-routes) enumera las claves de configuración y sus
valores por defecto; el [tutorial de niveles de Jev](../get-started/tutorials/jev-tiers.md)
construye paso a paso una ruta de tres niveles.

## Cómo se toma una decisión {#how-a-decision-is-made}

Antes de servir una petición, Synapse envía a Jev un resumen acotado de la conversación:

- el último mensaje del usuario, hasta 16,000 caracteres (los mensajes más largos conservan
  su principio y su final);
- el prompt de sistema, hasta 2,000 caracteres;
- tanto historial reciente como quepa en un presupuesto de unos 24,000 caracteres en total,
  con cada mensaje recortado a 2,000 caracteres;
- si la petición lleva herramientas y si lleva imágenes o medios.

Jev responde a dos preguntas sobre él:

1. **Dificultad**: una puntuación frente a las descripciones de tus niveles, en orden. Synapse
   redondea la puntuación al nivel más cercano.
2. **Necesidad de razonamiento**: la probabilidad de que una buena respuesta requiera
   razonamiento paso a paso, como matemáticas, lógica, planificación o depuración. Si alcanza
   o supera `reasoning_threshold` (por defecto 0.7), el esfuerzo del nivel sube un paso.

Si la confianza de Jev en la puntuación de dificultad está por debajo de `min_confidence`
(por defecto 0.5), la petición va a `default_tier`; la respuesta sobre razonamiento puede
seguir subiendo el esfuerzo. La llamada de decisión está limitada por `timeout_ms` (por
defecto 400 ms), lo que puede añadir hasta ese tiempo de latencia a cada petición de la ruta.

## Escribir buenos niveles {#write-good-tiers}

- **Describe el trabajo, nunca el modelo.** Jev puntúa la petición frente a las descripciones,
  así que "Multi-step analysis, maths or proofs, non-trivial code or debugging" funciona;
  "Gemini Pro" no.
- **Ordena los niveles del más fácil al más difícil.** El orden es lo que indexa la puntuación
  de dificultad, y determina el [fallback entre niveles](#tier-fallback).
- **Mantén los niveles diferenciados.** Las descripciones que se solapan hacen ambigua la
  puntuación, así que es mejor tener pocos niveles claramente distintos que muchos parecidos.
  Una ruta admite de 2 a 10.
- **Elige `default_tier` pensando en la seguridad.** Sirve siempre que Jev no puede decidir,
  así que escoge un nivel que atienda aceptablemente la mayoría de las peticiones,
  normalmente uno intermedio.

## Esfuerzo {#effort}

Cada nivel fija un `effort` de razonamiento, y Synapse lo traduce para cada proveedor y
carril. [Esfuerzo](../configuration/routes.md#effort) en la referencia de rutas tiene la tabla
de traducción. Cómo elegir el esfuerzo:

- **Niveles fáciles: `none` o `minimal`.** Los saludos, las búsquedas y las reescrituras no
  ganan nada con el razonamiento, y los tokens de razonamiento se facturan como salida. `none`
  no envía ningún esfuerzo, así que se aplica el valor por defecto del modelo; en algunos
  modelos Gemini ese valor es el razonamiento dinámico, así que usa `minimal` cuando quieras
  mantener el razonamiento pequeño.
- **Niveles intermedios: `low`.** Las preguntas cotidianas y las ediciones de código sencillas
  se benefician de un poco de razonamiento sin añadir mucha latencia.
- **Niveles difíciles: `medium` o `high`.** El análisis en varios pasos, las matemáticas y la
  depuración son donde el razonamiento compensa. Reserva `xhigh` y `max` para niveles en los
  que la calidad de la respuesta importa mucho más que la latencia y el coste.
- **Deja margen para la subida.** Cuando la probabilidad de razonamiento de Jev alcanza
  `reasoning_threshold` (por defecto 0.7), el esfuerzo del nivel sube un paso en la lista
  `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, y se detiene en `max`. Un nivel
  `medium` sirve las peticiones con mucho razonamiento a `high`, así que rara vez necesitas
  fijar el esfuerzo de un nivel en el máximo de lo que pagarías. Sube `reasoning_threshold`
  para subir el esfuerzo con menos frecuencia, o bájalo para hacerlo más a menudo.

El esfuerzo propio del cliente siempre gana en su carril:

- En el carril estándar, un `reasoning_effort` en el cuerpo de la petición sustituye el
  esfuerzo del nivel.
- En el carril Vertex nativo, un `vertex.thinking_config` sustituye el `thinkingBudget` del
  nivel. El carril nativo ignora `reasoning_effort`, así que enviar solo `reasoning_effort` en
  una petición nativa mantiene el presupuesto del nivel.

Cuando se aplica el esfuerzo del cliente, Jev sigue eligiendo el nivel y la respuesta indica
`x-synapse-reasoning-effort: client`.

## Fallback entre niveles {#tier-fallback}

Primero se ejecutan los tramos del nivel elegido, en orden. Si fallan todos, Synapse prueba
cada nivel más difícil, empezando por el más cercano, y después cada nivel más fácil,
empezando por el más cercano. En una ruta de cuatro niveles en la que Jev eligió el segundo,
el orden es niveles 2, 3, 4, 1. Cada tramo conserva el esfuerzo de su propio nivel.

Las peticiones con [funcionalidades nativas de Vertex](native-vertex.md) solo pueden usar
tramos `vertex`. Si el nivel elegido no tiene ninguno, sirve la petición el nivel más cercano
que sí lo tenga, empezando por los más difíciles. Si ningún nivel tiene un tramo `vertex`, la
petición falla con `400`.

Lo que cuenta como fallo de un tramo depende del carril; consulta
[Cadenas de fallback](fallback-chains.md).

## Anular la decisión por petición {#override-the-decision-per-request}

- `"routing_strategy": "static"` omite Jev en una petición. Se sirve desde `default_tier` con
  el esfuerzo configurado de cada nivel, sin subirlo nunca, y hace fallback en el mismo orden.
  Úsalo en llamadas sensibles a la latencia o para comparar con un nivel fijo.
- `"routing_strategy": "jev"` se acepta en una ruta `jev` y no cambia nada; en una ruta
  estática devuelve `400`. `"routing_strategy": "static"` en una ruta estática no cambia nada.
  Cualquier valor distinto de `static` o `jev` devuelve `400`.
- Un bloque `jev`, con `questions` o `extract`, devuelve `400` en una ruta `jev`; el
  [carril Jev](jev-lane.md) necesita una ruta con tramos `typesafe`.

## Encabezados de respuesta {#response-headers}

Toda respuesta de chat completion lleva `x-synapse-routing`; los demás encabezados aparecen
cuando procede:

| Encabezado | Valor |
|---|---|
| `x-synapse-routing` | `jev` en una ruta `jev`, también cuando la decisión se degradó; `static-override` cuando el cliente envió `"routing_strategy": "static"`; `static` en rutas sin niveles. |
| `x-synapse-tier` | El nivel cuyo tramo sirvió la petición. |
| `x-synapse-tier-decided` | El nivel que eligió Jev, solo cuando sirvió un nivel distinto. |
| `x-synapse-reasoning-effort` | El esfuerzo con el que se ejecutó el tramo que sirvió la petición, o `client`. |
| `x-synapse-routing-degraded` | Por qué no se usó la decisión de Jev: `timeout`, `error`, `low_confidence` o `jev_unavailable`. |

Las respuestas con streaming informan del tramo que produjo el primer fragmento, ya que los
encabezados se envían antes del cuerpo.

## Comportamiento ante fallos {#failure-behaviour}

Un problema de Jev nunca hace fallar la petición. Cuando Jev no puede decidir, la petición va
a `default_tier`, con su esfuerzo configurado, y la respuesta lleva
`x-synapse-routing-degraded`:

| Motivo | Causa |
|---|---|
| `timeout` | Jev no respondió dentro de `timeout_ms`. |
| `error` | Un error de conexión, una respuesta no satisfactoria o una respuesta que Synapse no pudo interpretar. |
| `low_confidence` | Jev respondió con una confianza inferior a `min_confidence`. El esfuerzo aún puede subir. |
| `jev_unavailable` | El gateway no tiene cliente de Jev, lo que ocurre cuando una aplicación que lo embebe construye el gateway sin él. |

Las decisiones fallidas o con tiempo de espera agotado se anotan como advertencias con target
`synapse::routing`, y solo llevan el tipo de fallo (`timeout`, `transport`, `http_status`,
`unreadable_body` o `unparseable_answers`) y el estado HTTP o el tiempo de espera
configurado, nunca el cuerpo de la respuesta, que podría reproducir texto del cliente.

Si fallan los tramos del nivel que sirve la petición, entra en juego el
[fallback entre niveles](#tier-fallback). Cuando fallan todos los tramos de todos los niveles
elegibles, la petición falla como en una ruta estática: `502` con `all_legs_failed` en el
carril estándar, o `502` con `upstream_error` en el carril Vertex nativo.

### Sin clave de TypeSafe {#without-a-typesafe-key}

Una ruta `jev` referencia el proveedor `typesafe`, así que con la validación de proveedores
estricta por defecto el gateway se niega a arrancar sin `TYPESAFE_API_KEY`. Con la validación
permisiva arranca y degrada la ruta a una ruta estática cuyos tramos se ejecutan en el orden
`default_tier`, cada nivel más difícil y cada nivel más fácil. Los tramos conservan el
esfuerzo de su nivel, así que las respuestas llevan `x-synapse-routing: static` y
`x-synapse-reasoning-effort`, pero no `x-synapse-tier`. Consulta
[Proveedores](../configuration/providers.md#strict-and-lenient-validation).

## Filas del registro {#ledger-rows}

Cada decisión que responde Jev escribe su propia fila en el registro, con el proveedor
`typesafe`, el `jev_router.model` como modelo, el carril `jev` y el mismo `request_id` que la
fila del chat que enrutó. Se valora como `typesafe:<model>` en `pricing.toml`. Las decisiones
con tiempo de espera agotado, fallidas o anuladas no escriben fila. En los eventos publicados
en Pub/Sub o SNS, las filas de decisión tienen `op = "route_decision"`; las tablas de SQLite y
Postgres no tienen columna `op`, así que identifica ahí las decisiones por proveedor y carril.
Consulta [Registro de costes](cost-ledger.md).

## Métricas y logs {#metrics-and-logs}

| Métrica | Etiquetas | Descripción |
|---|---|---|
| `synapse_routing_decisions_total` | `route`, `tier`, `outcome` | Una por cada petición planificada en una ruta `jev`. `tier` es el nivel decidido (`default_tier` cuando se degrada); `outcome` es `decided`, `low_confidence`, `timeout`, `error` (incluido `jev_unavailable`) o `static_override`. |
| `synapse_routing_decision_duration_seconds` | `route` | Latencia de cada llamada a Jev realizada, incluidas las que agotan el tiempo de espera. |

Cada petición planificada emite también un evento `info` con target `synapse::routing` y el
mensaje `route planned`, que lleva el modo, el nivel decidido, el resultado, el esfuerzo, y la
puntuación de dificultad, la confianza y la probabilidad de razonamiento de Jev. Las
peticiones rechazadas con `400` o por los guardrails no emiten ni la métrica ni el evento.

Una proporción creciente de resultados `timeout` o `error` significa que las peticiones están
acabando en `default_tier` sin una decisión real; aumenta `timeout_ms` o comprueba la
disponibilidad de Jev.

## En un gateway embebido {#in-an-embedded-gateway}

Las aplicaciones que embeben el gateway leen la misma información de `Gateway::chat_routed`,
que devuelve un `RoutingReport` junto a la respuesta, o de `GuardedStream::routing()` en los
streams. Consulta
[Leer el informe de enrutamiento](embedding-as-library.md#read-the-routing-report).
