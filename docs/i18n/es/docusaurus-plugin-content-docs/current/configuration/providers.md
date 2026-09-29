---
sidebar_position: 2
title: Proveedores
description: Los ids de proveedor que puede usar un tramo de ruta, las variables de entorno que necesita cada uno y cómo tratan las credenciales ausentes la validación estricta y la permisiva.
---

Cada tramo de `routes.toml` nombra un `provider`. Synapse conoce cinco ids de proveedor, y cada
uno lee sus credenciales y su endpoint de variables de entorno. Al arrancar, el gateway
comprueba que cada proveedor al que hacen referencia tus rutas tiene lo que necesita.

## Ids de proveedor {#provider-ids}

| Id de proveedor | Llama a | Obligatorias | Opcionales |
|---|---|---|---|
| `vertex` | Google Vertex AI (Gemini) | `VERTEX_PROJECT_ID` (o el heredado `VERTEX_PROJECT`), más Application Default Credentials | `VERTEX_LOCATION` (por defecto `global`) |
| `openai` | OpenAI | `OPENAI_API_KEY` | `OPENAI_BASE_URL` (por defecto `https://api.openai.com/v1`) |
| `qwen` | Alibaba Cloud DashScope (Qwen) | `DASHSCOPE_API_KEY` | `DASHSCOPE_BASE_URL` (por defecto `https://dashscope-intl.aliyuncs.com/compatible-mode/v1`) |
| `oai_compat` | Cualquier servidor compatible con OpenAI, como vLLM, Ollama o TGI | `OAI_COMPAT_BASE_URL` | `OAI_COMPAT_API_KEY` |
| `typesafe` | TypeSafe System One (Jev) | `TYPESAFE_API_KEY` | `TYPESAFE_BASE_URL` (por defecto `https://api.typesafe.ai`) |

## `vertex` {#vertex}

Synapse se autentica en Vertex AI con Application Default Credentials: una clave de cuenta de
servicio indicada por `GOOGLE_APPLICATION_CREDENTIALS`, tus credenciales de
`gcloud auth application-default login`, o el servidor de metadatos en Google Cloud. Al
arrancar solo comprueba que el proyecto está definido. Las credenciales ausentes o no válidas
aparecen en la primera petición.

Los tramos de Vertex funcionan en dos carriles:

- En el **carril estándar**, Synapse llama al endpoint `global` de Vertex AI a través del crate
  `genai`. La `region` del tramo se ignora.
- En el **carril Vertex nativo**, Synapse llama a la API REST de Vertex en la `region` del
  tramo, o en `VERTEX_LOCATION` cuando el tramo no tiene ninguna. `global` usa
  `aiplatform.googleapis.com`, las multirregiones `us` y `eu` usan
  `aiplatform.us.rep.googleapis.com` y `aiplatform.eu.rep.googleapis.com`, y una región
  concreta como `us-central1` usa `us-central1-aiplatform.googleapis.com`.

## `openai`, `qwen` y `oai_compat` {#openai-qwen-and-oai_compat}

Estos proveedores hablan la API de chat completions de OpenAI y se ejecutan en el carril
estándar a través del crate `genai`. Apunta una variable de URL base a un proxy o a un endpoint
regional para cambiar adónde van las peticiones. `oai_compat` no necesita clave de API; define
`OAI_COMPAT_API_KEY` si tu servidor la comprueba.

## `typesafe` {#typesafe}

Los tramos `typesafe` se ejecutan en el carril Jev: responden a las peticiones que llevan un
bloque `jev` con preguntas tipadas. Una ruta con un tramo `typesafe` devuelve `400` a las
peticiones que no lo llevan.

Una ruta `strategy = "jev"` también hace referencia a `typesafe`, porque es Jev quien toma su
decisión de enrutamiento, aunque sus niveles no puedan usar tramos `typesafe`.
`TYPESAFE_API_KEY` también habilita el passthrough `POST /typesafe/v1/systemone`.

## Alias de embeddings {#embedding-aliases}

Los alias de embeddings, declarados bajo `[embeddings."<alias>"]` en `routes.toml`, solo
admiten los proveedores `vertex` y `openai`. La validación también cubre sus tramos. Consulta
la [guía de embeddings](../guides/embeddings.md).

## Validación estricta y permisiva {#strict-and-lenient-validation}

`SYNAPSE_PROVIDER_VALIDATION` decide qué ocurre cuando una ruta hace referencia a un proveedor
que este proceso no puede construir, porque su variable obligatoria no está definida o el id
de proveedor es desconocido:

- **`strict`** (el valor por defecto) se niega a arrancar y nombra la variable que falta, por
  ejemplo `route references provider 'openai' but OPENAI_API_KEY is unset`. Un gateway
  dedicado debería fallar al arrancar antes que servir una tabla de rutas que no puede cumplir.
- **`lenient`** descarta los tramos que no puede servir, registra una advertencia por cada
  proveedor y arranca. Una ruta conserva los tramos que le quedan; una ruta que se queda sin
  tramos desaparece, así que las peticiones a ella devuelven `404` con el código de error
  `model_not_found`.

La validación permisiva existe para tablas de rutas compartidas por varios procesos, donde un
tramo añadido para un consumidor no debería detener a los demás. También poda las rutas
`strategy = "jev"`: se eliminan tramos dentro de cada nivel, se descartan los niveles vacíos y
se vuelve a elegir `default_tier`. Si falta `TYPESAFE_API_KEY` o quedan menos de dos niveles,
la ruta se degrada a una ruta estática cuyos tramos se ejecutan en el orden `default_tier`,
cada nivel más difícil y después cada nivel más fácil. Consulta [Rutas](routes.md#jev-routes).

Cualquier valor de `SYNAPSE_PROVIDER_VALIDATION` distinto de `lenient` equivale a `strict`.
