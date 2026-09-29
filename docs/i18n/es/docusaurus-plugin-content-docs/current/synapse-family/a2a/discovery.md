---
sidebar_position: 4
title: Descubrimiento
description: Lista los agentes A2A registrados, obtén la agent card de un agente y resuelve el id de un agente a su endpoint a través de los endpoints A2A públicos del gateway.
---

Los clientes usan tres endpoints públicos para encontrar agentes. Son de solo lectura y se
sirven en el puerto de API del gateway. Ninguno contacta con los agentes: devuelven lo que se
registró.

## `GET /.well-known/a2a-agent-catalog.json` {#get-well-knowna2a-agent-catalogjson}

Lista todos los agentes que no han caducado:

```json
{
  "version": "1.0",
  "agents": [
    {
      "id": "invoice-agent",
      "name": "Invoice agent",
      "description": "Extracts line items from invoices",
      "card_url": "https://agents.example.com/invoice/.well-known/agent-card.json",
      "endpoint_url": "https://agents.example.com/invoice",
      "tags": ["finance"]
    }
  ]
}
```

`version` siempre es `"1.0"`. El catálogo no incluye las agent cards, y el orden de `agents` no
está definido, así que ordénalo tú si lo muestras.

## `GET /a2a/agents/{id}/.well-known/agent-card.json` {#get-a2aagentsidwell-knownagent-cardjson}

Devuelve la agent card del agente exactamente como se registró u obtuvo al arrancar:

```json
{
  "name": "Invoice agent",
  "description": "Extracts line items from invoices",
  "url": "https://agents.example.com/invoice",
  "version": "1.0",
  "skills": []
}
```

Devuelve `404` con el cuerpo vacío si el id es desconocido o ha caducado.

## `GET /a2a/agents/{id}/resolve` {#get-a2aagentsidresolve}

Devuelve el endpoint al que llamar, con la agent card, en una sola petición:

```json
{
  "id": "invoice-agent",
  "endpoint_url": "https://agents.example.com/invoice",
  "card_url": "https://agents.example.com/invoice/.well-known/agent-card.json",
  "card": {
    "name": "Invoice agent",
    "description": "Extracts line items from invoices",
    "url": "https://agents.example.com/invoice",
    "version": "1.0",
    "skills": []
  }
}
```

Devuelve `404` con el cuerpo vacío si el id es desconocido o ha caducado. Después, los clientes
hablan A2A directamente con `endpoint_url`; el gateway no está en ese camino.

## Caducidad {#expiry}

Un agente registrado con `ttl_seconds` desaparece de los tres endpoints una vez transcurrido
ese número de segundos desde su registro. La caducidad se comprueba cuando se llama a un
endpoint, así que no hay limpieza en segundo plano ni retraso: la primera petición tras el plazo
ya no ve el agente.

Para mantener listado un agente con un tiempo de vida, haz que su host lo vuelva a registrar
antes del plazo. Como el registro sigue la regla gana el primero que escribe, el host debe hacer
`DELETE` del agente y después `POST`, y el agente falta del catálogo entre las dos llamadas;
consulta [Ids duplicados](./admin-api.md#duplicate-ids).
