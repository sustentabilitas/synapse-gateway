---
sidebar_position: 2
title: Semilla estática
description: Siembra el registro A2A al arrancar el gateway desde a2a.toml, y cómo obtiene el gateway la agent card de cada agente.
---

Enumera agentes en un archivo semilla cuando deban estar en el catálogo desde el momento en que
arranca el gateway, en todas las instancias, sin que nadie llame a la API de administración.

## El archivo semilla {#the-seed-file}

El gateway lee el archivo indicado por `SYNAPSE_A2A_PATH`, por defecto `config/a2a.toml`
relativo a su directorio de trabajo (`/app/config/a2a.toml` en la imagen de Docker). Cada agente
es una tabla `[[a2a_agents]]`:

```toml
[[a2a_agents]]
id = "invoice-agent"
name = "Invoice agent"
description = "Extracts line items from invoices"
endpoint_url = "https://agents.example.com/invoice"
card_url = "https://agents.example.com/invoice/.well-known/agent-card.json"
tags = ["finance"]

[[a2a_agents]]
id = "ghg-emissions"
name = "GHG emissions"
description = "Estimates greenhouse gas emissions"
endpoint_url = "https://agents.example.com/ghg"
card_url = "https://agents.example.com/ghg/.well-known/agent-card.json"
tags = ["ghg", "emissions"]
ttl_seconds = 86400
```

`id`, `name`, `description`, `endpoint_url` y `card_url` son obligatorios; `tags` y
`ttl_seconds` son opcionales. A diferencia de un registro a través de la API de administración,
el archivo semilla no tiene campo `card`: el gateway obtiene la agent card por su cuenta. Todas
las claves se describen en
[Claves de configuración](../../reference/configuration-keys.md#a2atoml).

## Qué ocurre al arrancar {#what-happens-at-startup}

- **Sin archivo:** el registro arranca vacío y el gateway registra en el log que no encontró
  ningún archivo semilla.
- **Un archivo que no se puede leer o analizar:** el gateway no arranca.
- **En otro caso,** el gateway procesa los agentes uno tras otro, antes de empezar a servir:
  1. Obtiene la agent card con `GET card_url`, con 15 segundos por intento.
  2. Los errores de conexión, los tiempos de espera agotados y las respuestas `5xx` y `429` se
     reintentan hasta dos veces, con backoff exponencial a partir de 200 ms. Cualquier otro
     estado distinto de `2xx`, o un cuerpo que no sea JSON, falla de inmediato.
  3. Si no se puede obtener la agent card, el agente se omite con un aviso y el gateway sigue
     sin él. Un host de agentes que todavía está arrancando no puede impedir que el gateway se
     levante.
  4. En otro caso, el agente se registra con la agent card obtenida.

La agent card se obtiene una sola vez. El registro no la refresca después, así que un agente
cuya agent card cambie necesita un reinicio del gateway, o un `DELETE` y un `POST` a través de
la [API de administración](./admin-api.md).

Como la siembra es secuencial y bloquea el arranque, un host de agentes inaccesible añade hasta
unos 45 segundos al arranque. Limita el archivo a agentes cuyos hosts suelan estar disponibles.

## Reglas de registro {#registration-semantics}

El registro sigue la regla **gana el primero que escribe** (first-writer-wins), tanto para el
archivo semilla como para la API de administración:

- Un `id` que ya está registrado no se sustituye. Un `id` duplicado en el archivo semilla se
  omite con un aviso, y se queda la primera entrada.
- Para sustituir un agente, haz `DELETE` y vuelve a registrarlo.

## Caducidad {#expiry}

Con `ttl_seconds`, un agente sembrado sale del catálogo ese número de segundos después de que
arrancara el gateway. Omítelo para los agentes que deban permanecer durante toda la vida del
proceso. Consulta [Caducidad](./discovery.md#expiry).
