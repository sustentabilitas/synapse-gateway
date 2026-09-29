---
sidebar_position: 2
title: Publicar versiones
description: Cómo quienes mantienen el proyecto versionan y publican cada crate de Synapse en crates.io y Docker Hub con los workflows de bump y de publicación.
---

Cada crate del workspace se versiona y se publica por separado, con una etiqueta que lleva el
nombre del crate: `synapse-gateway-vX.Y.Z`, `synapse-proxy-vX.Y.Z`, `synapse-a2a-vX.Y.Z`,
`synapse-mcp-vX.Y.Z` o `synapse-context-vX.Y.Z`. Publicar versiones es tarea exclusiva de
quienes mantienen el proyecto.

## Qué publica una versión {#what-a-release-publishes}

| Crate | crates.io | Docker Hub | Workflow de publicación |
|---|---|---|---|
| `synapse-gateway` | Sí | `sustentabilitas/synapse-gateway` | `release.yml` |
| `synapse-proxy` | Sí | `sustentabilitas/synapse-proxy` | `release.yml` |
| `synapse-a2a` | Sí | No | `release-libs.yml` |
| `synapse-mcp` | Sí | No | `release-libs.yml` |
| `synapse-context` | Sí | No | `release-libs.yml` |

Para una etiqueta `<crate>-vX.Y.Z`, el workflow de publicación:

1. comprueba que la versión `X.Y.Z` del crate no está ya en crates.io y que la etiqueta coincide
   con la `version` de `crates/<crate>/Cargo.toml`;
2. ejecuta `cargo publish -p <crate> --locked`;
3. solo para el gateway y el proxy, compila el Dockerfile del crate para `linux/amd64` y sube
   `sustentabilitas/<crate>:X.Y.Z` y `:latest` a Docker Hub.

Por otro lado, cada push a `main` compila y sube las imágenes continuas
`sustentabilitas/synapse-gateway:edge` y `sustentabilitas/synapse-proxy:edge`, sin publicar
ningún crate.

## Publicar con el workflow de bump {#release-with-the-bump-workflow}

La forma habitual de publicar es el workflow **Bump & release** del crate, en la pestaña
Actions del repositorio: **Bump & release synapse-gateway**, **Bump & release synapse-proxy**,
etc. Elige `patch`, `minor` o `major`; el workflow siempre trabaja sobre `main`. Este workflow:

1. calcula la siguiente versión a partir del `Cargo.toml` del crate o de su última etiqueta, la
   que sea mayor;
2. actualiza la `version` en `crates/<crate>/Cargo.toml`, regenera `Cargo.lock` y añade un
   encabezado para la nueva versión en `crates/<crate>/CHANGELOG.md`;
3. hace el commit `chore(<crate>): release vX.Y.Z` en `main`, lo etiqueta como `<crate>-vX.Y.Z`
   y sube ambos;
4. lanza el workflow de publicación para esa etiqueta.

El workflow de bump lanza él mismo el workflow de publicación porque una etiqueta subida con el
token propio del workflow no dispara otros workflows.

## Publicar desde tu máquina {#release-from-your-machine}

Subir una etiqueta desde tu máquina dispara directamente el workflow de publicación. Primero
actualiza tú la versión y el registro de cambios, y después haz commit en `main`, etiqueta y sube:

```bash
git tag synapse-gateway-vX.Y.Z
git push origin synapse-gateway-vX.Y.Z
```

La publicación falla en la primera comprobación si la etiqueta no coincide con el `Cargo.toml`
del crate.

También puedes ejecutar **Release** o **Release libraries** a mano desde la pestaña Actions.
Desde una rama, publica la versión indicada en el `Cargo.toml` del crate y, para los binarios,
sube la imagen sin mover `latest`.

## Orden de publicación {#publish-order}

crates.io ya debe tener la versión de cada dependencia del workspace contra la que se publica un
crate, así que publica primero las dependencias:

- `synapse-context` antes que `synapse-proxy` y `synapse-mcp`, que dependen de él.
- `synapse-a2a` antes que `synapse-gateway`, que depende de él a través de la feature `server`.

## Secretos del repositorio {#repository-secrets}

Los workflows de publicación necesitan estos secretos, en
**Settings → Secrets and variables → Actions**:

| Secreto | Uso |
|---|---|
| `CARGO_REGISTRY_TOKEN` | Publicar en crates.io |
| `DOCKERHUB_USERNAME` | Subir imágenes a Docker Hub |
| `DOCKERHUB_TOKEN` | Subir imágenes a Docker Hub |

Consulta los [registros de cambios](./changelog.md) para ver qué contiene cada versión.
