---
sidebar_position: 4
title: Precios
description: Referencia de pricing.toml, los precios por modelo que usa Synapse para calcular el coste de cada petición en el registro de costes.
---

`pricing.toml` contiene el precio de cada modelo que usan tus rutas. Synapse multiplica los
recuentos de tokens de una petición por estos precios para registrar su coste en el registro
de costes.

El gateway lee el archivo de `SYNAPSE_PRICING_PATH` (por defecto `config/pricing.toml`) al
arrancar y se niega a iniciarse si falta o no es válido. Un archivo vacío es válido: en ese
caso, todas las peticiones cuestan 0.

## Formato {#format}

Cada entrada se indexa por `provider:model` y tiene un precio `input` y otro `output` en USD
por 1,000,000 de tokens. Ambos precios son obligatorios.

```toml title="config/pricing.toml"
"vertex:gemini-3.5-flash-lite" = { input = 0.30, output = 2.50 }
"vertex:gemini-3.6-flash" = { input = 1.50, output = 7.50 }
"openai:gpt-4o-mini" = { input = 0.15, output = 0.60 }
"typesafe:jev-latest" = { input = 0.042, output = 0.0 }
```

Los mismos precios escritos como tablas TOML, la forma que usa el
[`config/pricing.toml`](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-gateway/config/pricing.toml)
incluido:

```toml title="config/pricing.toml"
["vertex:gemini-3.5-flash-lite"]
input = 0.30
output = 2.50

["vertex:gemini-3.6-flash"]
input = 1.50
output = 7.50

["openai:gpt-4o-mini"]
input = 0.15
output = 0.60

["typesafe:jev-latest"]
input = 0.042
output = 0.0
```

## Cómo se emparejan las claves {#how-keys-are-matched}

- `provider` es el id de proveedor del tramo: `vertex`, `openai`, `qwen`, `oai_compat` o
  `typesafe`.
- `model` es el `model` del tramo exactamente como está escrito en `routes.toml`, no el alias
  de la ruta. Una ruta con dos tramos necesita dos entradas.
- Las decisiones de enrutamiento de Jev se valoran como `typesafe:<model>`, donde `<model>` es
  el `jev_router.model` de la ruta (por defecto `jev-latest`).

## Cómo se calcula el coste {#how-cost-is-computed}

Para cada petición completada:

```text
cost_usd = (input_tokens × input + output_tokens × output) / 1,000,000
```

Un modelo de chat sin entrada cuesta 0 y la petición se completa igualmente, algo adecuado
para modelos autoalojados. Un modelo de embeddings sin entrada se valora a
`SYNAPSE_EMBED_DEFAULT_INPUT_PRICE_PER_MTOK` (por defecto `0.10`) por 1,000,000 de tokens de
entrada, para que el uso de embeddings nunca salga gratis sin que te des cuenta.

Los recuentos de tokens proceden de la respuesta del proveedor. Ten en cuenta dos detalles del
carril Vertex nativo:

- `input_tokens` incluye los tokens leídos de una caché de contexto, y Synapse los valora al
  precio `input` completo. Vertex AI factura los tokens en caché con descuento, así que el
  registro de costes sobrestima el coste de las peticiones con caché.
- `output_tokens` es el `candidatesTokenCount` de Vertex AI, que no incluye los tokens de
  razonamiento. Vertex AI factura los tokens de razonamiento como salida, así que el registro
  de costes subestima el coste de las peticiones que razonan.

Los precios cambian; consulta la lista de precios de cada proveedor y actualiza el archivo
cuando lo hagan.
