---
sidebar_position: 7
title: Atribución por tenant
description: Atribuye cada petición a un tenant, workspace, usuario, hilo y tipo de tarea con encabezados x-synapse-*, y clasifica el trabajo con tipos de tarea de IA.
---

Usa la atribución por tenant cuando más de un equipo, cliente o aplicación comparte un gateway
y necesitas saber quién gastó qué. Los clientes añaden encabezados `x-synapse-*` a sus
peticiones, y Synapse los anota en cada fila del [registro de costes](cost-ledger.md), de modo
que puedes informar del uso y del coste por tenant, por workspace, por usuario final o por
tipo de trabajo sin cambiar el cuerpo de la petición.

## Encabezados de atribución {#attribution-headers}

| Encabezado | Se registra como | Descripción |
|---|---|---|
| `x-synapse-tenant` | `tenant` | El cliente, equipo o aplicación. Sin el encabezado, `SYNAPSE_DEFAULT_TENANT` (por defecto `unattributed`). |
| `x-synapse-workspace` | `workspace` | Una agrupación dentro del tenant, como un proyecto o un equipo. |
| `x-synapse-user` | `user_id` | El usuario final dentro del tenant. |
| `x-synapse-thread` | `thread_id` | Una conversación o un hilo de agente. |
| `x-synapse-message` | `message_id` | Un mensaje o unidad de trabajo dentro del hilo. También se convierte en el id de la petición; consulta [Correlaciona peticiones](#correlate-requests). |
| `x-synapse-user-task-type` | `user_task_type` | Tu propia etiqueta para el trabajo al que sirve la petición, como `summarisation`. Se registra tal cual se envía y nunca se interpreta. |
| `x-synapse-ai-task-type` | `ai_task_type` | Sustituye el [tipo de tarea de IA](#ai-task-types) para esta petición. |

Todos los encabezados son opcionales. Funcionan igual en `POST /v1/chat/completions`,
`POST /v1/embeddings` y los endpoints passthrough. Por ejemplo:

```bash
curl -s http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "x-synapse-tenant: my-team" \
  -H "x-synapse-workspace: onboarding" \
  -H "x-synapse-user: user-42" \
  -H "x-synapse-user-task-type: summarisation" \
  -d '{
    "model": "chat",
    "messages": [{"role": "user", "content": "Summarise our onboarding guide."}]
  }'
```

Synapse registra los valores tal cual se envían. Un encabezado presente pero vacío registra
una cadena vacía en lugar de recurrir al valor por defecto.

:::warning
Synapse no autentica estos encabezados: cualquier cliente puede declarar cualquier tenant. Si
la atribución determina la facturación o las cuotas, ejecuta Synapse detrás de un gateway o
una service mesh que autentique a quienes llaman y establezca `x-synapse-tenant` por sí mismo,
sobrescribiendo cualquier valor que haya enviado el cliente.
:::

## Adónde va la atribución {#where-attribution-goes}

Cada fila del registro tiene una columna para cada uno de los siete campos. `tenant` y
`ai_task_type` siempre tienen valor; los demás son nulos cuando no se envió el encabezado. Los
eventos publicados en Pub/Sub o SNS llevan los mismos campos en camelCase, omitiendo los
ausentes, con el tenant como `namespace`. Consulta [Registro de costes](cost-ledger.md) para el
formato completo de filas y eventos.

Ninguno de ellos es una etiqueta de métrica. Sus valores vienen de los clientes y no están
acotados, y ponerlos en las métricas crearía una serie nueva por cada tenant, usuario o hilo.
Consulta el registro para obtener cifras por tenant, y usa las métricas para vistas por ruta,
por modelo y por carril.

Da a `SYNAPSE_DEFAULT_TENANT` un valor reconocible, como el nombre del despliegue, para que el
uso sin atribuir destaque. Consulta
[Variables de entorno](../configuration/environment-variables.md#tenancy).

## Correlaciona peticiones {#correlate-requests}

Cada petición recibe un id de petición, que se registra como `request_id` en el registro. En
los chat completions es también el `id` de la respuesta, como `chatcmpl-<request id>`. Cuando
la petición tiene un encabezado `x-synapse-message`, ese valor es el id de la petición; si no,
Synapse genera un UUID.

Estampar `x-synapse-message` te permite unir una fila del registro con el mensaje en tu propio
sistema. Todas las filas que escribe una misma petición comparten su id de petición: en una
ruta `strategy = "jev"`, la decisión de enrutamiento y el chat completion; con extracción
híbrida, la respuesta de Jev y cada extracción. Reutilizar un mismo id de mensaje en varias
peticiones da a sus filas el mismo `request_id`.

## Tipos de tarea de IA {#ai-task-types}

El tipo de tarea de IA clasifica el trabajo que realizó el gateway, como `conversation`,
`extraction` o `vision`, para que puedas informar del coste por tipo de trabajo entre rutas y
tenants. Cada fila del registro tiene uno. Synapse lo resuelve por petición:

1. el encabezado `x-synapse-ai-task-type`, si está presente y no está vacío;
2. si no, el tipo de tarea asignado al alias de ruta de la petición en `ai_task_types.toml`;
3. si no, `simple`.

`ai_task_types.toml` se indexa por tipo de tarea, y cada uno enumera los alias de ruta que
realizan ese tipo de trabajo:

```toml
conversation = ["chat", "support-bot"]
extraction = ["invoice-extract"]
vision = ["receipt-ocr"]
```

- El gateway lo lee de `SYNAPSE_AI_TASK_TYPES_PATH` (por defecto
  `config/ai_task_types.toml`) al arrancar. El fichero es opcional: sin él, todas las
  peticiones se resuelven a `simple` salvo que el encabezado diga otra cosa.
- Los alias no listados se resuelven a `simple`, así que enumera solo las rutas cuyo trabajo
  no es simple.
- Un alias listado bajo dos tipos de tarea detiene el gateway al arrancar; listarlo dos veces
  bajo el mismo tipo está permitido.
- En los embeddings, el alias es el alias de embeddings.

Los dos campos de tipo de tarea responden a preguntas distintas. `user_task_type` es la
etiqueta propia de tu aplicación, y solo se registra cuando un cliente la envía.
`ai_task_type` siempre tiene valor y se controla de forma centralizada en
`ai_task_types.toml`, así que los informes se mantienen coherentes aunque los clientes no
envíen nada.

## En un gateway embebido {#in-an-embedded-gateway}

Las aplicaciones que embeben el gateway pasan los mismos campos en un `RequestCtx`, cuyos
campos `tenant`, `workspace`, `user`, `thread`, `message`, `user_task_type` y `ai_task_type`
corresponden a los encabezados, más un `request_id` opcional. Consulta
[Synapse como biblioteca embebida](embedding-as-library.md).
