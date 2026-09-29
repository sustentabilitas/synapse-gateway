---
sidebar_position: 5
title: Hoja de ruta de MCP
description: Lo que synapse-mcp todavía no soporta.
---

La primera versión de `synapse-mcp` enruta cada servidor MCP por separado y solo reenvía
herramientas. Todavía no se soporta:

- **Agregación de herramientas entre servidores.** No hay una superficie de herramientas
  combinada. Cada servidor se alcanza en su propio path `/mcp/<server>`, y un cliente que
  necesita varios servidores se conecta a cada path.
- **Compatibilidad con SSE.** Los servidores upstream deben hablar Streamable HTTP; los
  servidores que solo soportan el transporte heredado HTTP+SSE no se pueden registrar.
- **Varias fijaciones de identidad concurrentes.** Solo hay una identidad activa por proceso a
  la vez, en consonancia con el `ContextStore` de `synapse-context`.
- **Recursos, prompts y otras capacidades.** Solo se reenvían `tools/list` y `tools/call`.
- **Configuración estática.** No hay archivo semilla ni binario; los servidores se registran a
  través de las rutas de administración o desde código. Consulta [Registro](./registration.md).

Las mismas carencias figuran junto a las del resto de Synapse en
[Limitaciones y hoja de ruta](../../reference/limitations-roadmap.md#synapse-mcp).
