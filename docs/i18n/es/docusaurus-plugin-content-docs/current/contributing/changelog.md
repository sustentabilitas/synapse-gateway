---
sidebar_position: 3
title: Registro de cambios
description: Dónde encontrar el registro de cambios de cada crate de Synapse.
---

Cada crate mantiene su propio registro de cambios (changelog), que se actualiza en cada pull
request que cambia el crate y recibe un encabezado de versión cuando el crate se
[publica](./releasing.md):

| Crate | Registro de cambios |
|---|---|
| `synapse-gateway` | [crates/synapse-gateway/CHANGELOG.md](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-gateway/CHANGELOG.md) |
| `synapse-proxy` | [crates/synapse-proxy/CHANGELOG.md](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-proxy/CHANGELOG.md) |
| `synapse-a2a` | [crates/synapse-a2a/CHANGELOG.md](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-a2a/CHANGELOG.md) |
| `synapse-mcp` | [crates/synapse-mcp/CHANGELOG.md](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-mcp/CHANGELOG.md) |
| `synapse-context` | [crates/synapse-context/CHANGELOG.md](https://github.com/sustentabilitas/synapse-gateway/blob/main/crates/synapse-context/CHANGELOG.md) |

Las versiones publicadas también aparecen en crates.io, por ejemplo
[synapse-gateway](https://crates.io/crates/synapse-gateway/versions), y las imágenes del gateway
y del proxy se etiquetan con las mismas versiones en
[Docker Hub](https://hub.docker.com/r/sustentabilitas/synapse-gateway/tags).
