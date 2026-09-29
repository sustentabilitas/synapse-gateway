---
sidebar_position: 1
title: Contribuir
description: Cómo compilar y probar el workspace de Synapse, preparar una pull request, firmar tus commits y trabajar en este sitio de documentación.
---

Synapse es un proyecto de código abierto con licencia AGPL-3.0, y las incidencias, los informes
de errores, las solicitudes de funcionalidades y las pull requests son bienvenidos. Antes de
empezar, lee el
[Código de conducta](https://github.com/sustentabilitas/synapse-gateway/blob/main/CODE_OF_CONDUCT.md)
y la [política de seguridad](../operating/security.md). Al participar, aceptas cumplir el
Código de conducta.

## Primeros pasos {#getting-started}

### Requisitos previos {#prerequisites}

- La [toolchain de Rust](https://rustup.rs/) estable actual, con `rustfmt` y `clippy` (ambos se
  instalan con `rustup`). El repositorio no fija ninguna toolchain; la CI usa la última estable.
- Opcional: Docker, para compilar las imágenes del gateway y del proxy.
- Opcional: Node.js 22, para trabajar en este sitio de documentación.

### Clonar y compilar {#clone-and-build}

```bash
git clone https://github.com/sustentabilitas/synapse-gateway.git
cd synapse-gateway

# Compila todos los crates del workspace (features por defecto del gateway: server + ledger-sqlite)
cargo build

# Ejecuta la batería de tests
cargo test
```

El workspace tiene cinco crates en `crates/`; consulta
[Crates del workspace](../internals/workspace-crates.md) para ver qué hace cada uno.

### Matriz de features {#feature-matrix}

El gateway tiene backends opcionales para el registro de costes y una compilación ligera como
biblioteca. Cuando tu cambio afecte a un backend del registro de costes o a la superficie de la
biblioteca embebible, compila las variantes afectadas:

| Comando | Qué habilita |
|---|---|
| `cargo build -p synapse-gateway` | Features por defecto (`server` + `ledger-sqlite`) |
| `cargo build -p synapse-gateway --features ledger-postgres` | Destino PostgreSQL del registro de costes |
| `cargo build -p synapse-gateway --features ledger-pubsub` | Destino Google Cloud Pub/Sub del registro de costes |
| `cargo build -p synapse-gateway --features ledger-sns` | Destino AWS SNS del registro de costes |
| `cargo build -p synapse-gateway --features "ledger-pubsub ledger-sns"` | Ambos destinos en la nube a la vez |
| `cargo build -p synapse-gateway --no-default-features --lib` | Núcleo embebible ligero (sin servidor HTTP ni registro de costes) |

## Antes de enviar {#before-you-submit}

Ejecuta estas comprobaciones en local antes de abrir una pull request. La CI ejecuta cada una de
ellas, y una comprobación fallida bloquea el merge.

```bash
# 1. Formato (no debe producir ningún diff)
cargo fmt --all --check

# 2. Lints, con los avisos tratados como errores
cargo clippy --all-targets -- -D warnings

# 3. Tests, con las features por defecto
cargo test

# 4. Las variantes de features que afecta tu cambio
cargo build -p synapse-gateway --features ledger-postgres
cargo build -p synapse-gateway --features ledger-pubsub
cargo build -p synapse-gateway --features ledger-sns
cargo build -p synapse-gateway --features "ledger-pubsub ledger-sns"
cargo build -p synapse-gateway --no-default-features --lib
```

Además:

- **Actualiza el registro de cambios.** Cada crate tiene su propio
  `crates/<crate>/CHANGELOG.md`. Para el gateway, añade una línea bajo `## [Unreleased]`, en el
  grupo `Added`, `Changed`, `Fixed`, `Removed` o `Security`
  ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)); el workflow de publicación
  convierte esa sección en la nueva versión. Para los demás crates, el workflow de publicación
  añade el encabezado de la nueva versión al principio del archivo, así que añade tu línea ahí
  cuando publiques (consulta [Publicar versiones](releasing.md)).
- **Actualiza la documentación.** Si tu cambio afecta a la API pública, la configuración, los
  endpoints HTTP, las métricas o el comportamiento, actualiza las páginas de este sitio (consulta
  [Trabajar en la documentación](#working-on-the-docs)) y los comentarios de rustdoc.

## Flujo de desarrollo {#development-workflow}

Las contribuciones no triviales, como funcionalidades nuevas, refactorizaciones importantes,
nuevos backends del registro de costes o cambios en la API pública de la biblioteca, siguen un
flujo de **especificación, plan e implementación**:

1. **Escribe una especificación.** Describe *qué* y *por qué*: el problema, el comportamiento
   propuesto, los casos límite y los criterios de aceptación. Que sea breve.
2. **Escribe un plan.** Divide el trabajo en pasos pequeños y revisables que hagan referencia a
   la especificación.
3. **Implementa con TDD.** Escribe primero el test que falla, en `tests/` o en un módulo
   `#[cfg(test)]` junto al código; después, la implementación mínima que lo haga pasar, y por
   último refactoriza. Haz commit del test por separado de la implementación cuando eso facilite
   la revisión.
4. **Abre una pull request** que enlace o resuma la especificación y el plan, para que quienes
   revisen tengan todo el contexto.

Las especificaciones y los planes son documentos de trabajo: no los incluyas en commits bajo
`docs/`, que contiene este sitio. Las correcciones de errores pequeñas y las mejoras de la
documentación no necesitan especificación; usa tu criterio.

## Mensajes de commit {#commit-messages}

- Escribe una **línea de asunto en imperativo y en presente**, como `add Pub/Sub ledger sink`, no
  `added` ni `adding`.
- Mantén el asunto por debajo de **72 caracteres**.
- Usa un prefijo de [conventional commits](https://www.conventionalcommits.org/), con el ámbito
  del crate que cambias cuando sea útil, por ejemplo `fix(synapse-proxy): ...`:

  | Prefijo | Uso |
  |---|---|
  | `feat:` | Funcionalidad o comportamiento nuevo |
  | `fix:` | Corrección de un error |
  | `docs:` | Solo cambios en la documentación |
  | `refactor:` | Reestructuración del código sin cambios de comportamiento |
  | `test:` | Añadir o actualizar tests |
  | `chore:` | Mantenimiento, actualización de dependencias, herramientas |
  | `perf:` | Mejoras de rendimiento |
  | `ci:` | Cambios en el pipeline de CI/CD |

- Si quieres, añade un cuerpo, tras una línea en blanco, que explique *por qué* hiciste el cambio.
- Haz referencia a las incidencias o pull requests relacionadas al final, por ejemplo `Closes #42`.

```text
feat(synapse-gateway): add AWS SNS ledger sink

Adds a fan-out sink that publishes cost-ledger events to an SNS topic.
Gated behind the `ledger-sns` feature flag.

Closes #17
Signed-off-by: Your Name <your@email.com>
```

## Developer Certificate of Origin {#developer-certificate-of-origin}

**Cada commit debe llevar un trailer `Signed-off-by`.** Al firmarlo, certificas que tienes
derecho a enviar la contribución bajo la licencia AGPL-3.0 del proyecto, tal como define el
[Developer Certificate of Origin](https://developercertificate.org/).

Añade la firma con la opción `-s`:

```bash
git commit -s -m "feat: your change description"
```

Esto añade una línea como la siguiente, con tu nombre real y una dirección de correo que
funcione:

```text
Signed-off-by: Your Name <your@email.com>
```

:::warning
Las pull requests que contienen commits sin firmar no se fusionan. Si olvidaste firmar commits
anteriores, modifícalos:

```bash
# El commit más reciente
git commit --amend -s --no-edit

# Todos los commits de la rama
git rebase --signoff HEAD~<N>
```
:::

## Pull requests {#pull-requests}

1. Haz un **fork** del repositorio y crea una rama de funcionalidad a partir de `main`.
2. Sigue el [flujo de desarrollo](#development-workflow) y las
   [pautas para mensajes de commit](#commit-messages).
3. Asegúrate de que **todas las comprobaciones de CI pasan** antes de pedir una revisión.
4. Abre una pull request con:
   - un título claro, al estilo de conventional commits;
   - una descripción de *qué* ha cambiado y *por qué*;
   - enlaces a la especificación y al plan en los cambios no triviales;
   - `Closes #<issue>` si corresponde.
5. Atiende los comentarios de la revisión con prontitud. Para fusionar hace falta una revisión
   aprobatoria de una persona mantenedora.
6. Quienes mantienen el proyecto pueden hacer squash o rebase al fusionar para mantener limpio el
   historial.

## Trabajar en la documentación {#working-on-the-docs}

Este sitio es un proyecto de [Docusaurus](https://docusaurus.io/) en el directorio `docs/`, en
inglés y español. Para ejecutarlo en local con recarga en vivo:

```bash
cd docs && npm ci && npm start
```

`npm start` sirve un solo idioma cada vez. Para previsualizar el sitio en español:

```bash
npm start -- --locale es
```

Antes de abrir una pull request que toque `docs/`, ejecuta las mismas comprobaciones que la CI:

```bash
npm test              # tests unitarios de los scripts del sitio
npm run check:i18n    # cada página en inglés tiene su gemela en español
npm run typecheck
npm run build         # compila ambos idiomas; falla con enlaces y anclas rotos
```

Las páginas son archivos Markdown (`.md`) en `docs/docs/`, con front matter `sidebar_position`,
`title` y `description`, y enlaces relativos que incluyen la extensión `.md`. Dos reglas
mantienen el sitio fiable:

- **Paridad en español.** Una pull request que añade o cambia una página en inglés actualiza su
  gemela en español en `docs/i18n/es/docusaurus-plugin-content-docs/current/`, en la misma ruta
  relativa. La CI ejecuta `npm run check:i18n`, que falla cuando una página existe en un idioma y
  no en el otro.
- **Los ejemplos de configuración con título se prueban.** Un bloque de código cuyo título
  termina en `routes.toml`, `pricing.toml`, `guardrails.toml` o `ai_task_types.toml`, como
  ```` ```toml title="config/routes.toml" ````, debe ser un archivo completo y válido. Los propios
  parsers del gateway cargan cada uno de ellos en ambos idiomas cuando ejecutas
  `cargo test -p synapse-gateway --test docs_examples`, que forma parte de `cargo test`. Delimita
  los fragmentos parciales con un simple ```` ```toml ```` sin título.

`docs/superpowers/` está en el gitignore y es solo local: guarda ahí tus notas de trabajo y no
hagas nunca commit de nada que esté dentro.

## Licencia {#license}

Al enviar una contribución, aceptas que tu trabajo se licencie bajo la
[GNU Affero General Public License v3.0](https://github.com/sustentabilitas/synapse-gateway/blob/main/LICENSE)
(AGPL-3.0), la misma licencia que el resto del proyecto. Si tienes alguna pregunta, abre una
discusión en GitHub o usa el contacto de la [política de seguridad](../operating/security.md).
