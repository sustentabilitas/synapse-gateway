---
slug: introducing-axonal
title: "Presentamos axonal: un único ejecutor de tareas para Cargo y pnpm"
authors: [rajwilkhu]
tags: [tooling]
date: 2026-10-01T10:00
description: axonal es un ejecutor de tareas experimental para monorepos, escrito en Rust, que infiere los proyectos a partir de los manifiestos de Cargo y pnpm, cachea cada tarea y ejecuta solo lo que un cambio puede afectar. Synapse ya lo usa.
---

Synapse es un workspace de Cargo con cinco crates y, a su lado, un sitio de documentación con
npm, y hasta ahora cada pull request ejecutaba todas las comprobaciones de Rust: el formato,
clippy y los tests de los cinco crates, y cinco compilaciones con features, tanto si el cambio
tocaba un crate como si corregía una errata de la documentación.
[axonal](https://github.com/sustentabilitas/axonal) es el ejecutor de tareas que hemos creado
para evitarlo. Su comando es `ax` y, desde hoy, este repositorio tiene un `axonal.toml`.

axonal es experimental. Este artículo explica qué hace, cómo decide qué ejecutar y qué viene
después.

<!-- truncate -->

## Por qué otro ejecutor de tareas {#why-another-task-runner}

La mayoría de los ejecutores de tareas para monorepos nacieron en torno a JavaScript, y para
ellos Cargo es un añadido. Queríamos tres cosas de una sola herramienta: que los repositorios
mixtos de Cargo y pnpm fueran ciudadanos de primera, una configuración que cupiera en un único
archivo pequeño y un modelo del workspace lo bastante preciso para decir qué comprobaciones
puede romper un cambio. axonal es un único binario de Rust que hoy cumple las dos primeras y
sienta las bases de la tercera.

## Lee los manifiestos que ya tienes {#it-reads-the-manifests-you-already-have}

axonal encuentra los proyectos igual que tus herramientas de compilación. Cada miembro del
workspace de Cargo es un proyecto, que se encuentra con `cargo metadata`, y recibe los targets
`build`, `test`, `lint` (clippy) y `fmt`. Cada paquete del workspace de pnpm es un proyecto
cuyos scripts de `package.json` son sus targets. Las dependencias entre ellos se convierten en
aristas, igual que los imports de TypeScript entre proyectos. Todo lo demás se declara en
`axonal.toml` con comandos de shell normales. El sitio de documentación de Synapse usa npm en
lugar de pnpm, así que se declara ahí:

```toml
[projects.docs.targets.test]
command = "npm test"
```

`ax graph` construye el grafo completo de este repositorio en unos 30 milisegundos.

## Cada tarea tiene una clave {#every-task-has-a-key}

Antes de ejecutar una tarea, axonal calcula un hash de todo lo que puede cambiar su resultado:
el comando, los archivos de entrada del proyecto, los archivos de lockfile y de toolchain, los
archivos correspondientes de cada proyecto del que depende, las variables de entorno indicadas
y las versiones de `rustc`, `node` y `pnpm`. Si la caché local ya contiene una ejecución
correcta para esa clave, axonal reproduce los logs y restaura las salidas en lugar de
ejecutarla. Ejecuta `ax run fmt lint test` dos veces y la segunda solo informará de aciertos de
caché.

Incluir en el hash los archivos de las dependencias importa en Cargo más de lo que parece.
`cargo test -p synapse-gateway` no tiene una dependencia de orden sobre `synapse-a2a`, porque
Cargo lo compila como parte de los tests, pero un cambio en `synapse-a2a` tiene que volver a
ejecutar los tests del gateway. axonal incluye el código de la dependencia en la clave del
gateway, así que lo hace.

## Solo lo que un cambio puede afectar {#only-what-a-change-can-affect}

`ax run test --affected` compara tu rama con el merge-base sobre `main`, asigna cada archivo
cambiado a su proyecto y a los targets cuyas entradas coinciden con él, y añade todos los
proyectos que dependen de ellos. Un `Cargo.lock` cambiado se analiza en sus dos versiones y se
compara, así que actualizar una dependencia afecta solo a los proyectos que realmente la usan.

El conjunto tiene una regla que nunca debe romper: es una cota superior. Ejecutar una
comprobación que no hacía falta cuesta un minuto; saltarse una que sí hacía falta deja entrar un
cambio roto. Un test de propiedades de axonal edita archivos al azar en workspaces aleatorios y
comprueba que cada tarea cuya clave de caché ha cambiado está en el conjunto de afectadas.

Este repositorio muestra por qué importa un modelo preciso. El test `docs_examples` del gateway
analiza los ejemplos de configuración de este sitio, así que un cambio solo en la documentación
puede romper un test de Rust. Una línea en `axonal.toml` le dice a axonal que el test lee la
documentación:

```toml
[projects."crates/synapse-gateway".targets.test]
inputs = ["**/*", "{workspace}/docs/docs/**", "{workspace}/docs/i18n/**"]
```

Ahora, editar una página afecta a los tests del sitio de documentación y a los del gateway, y a
nada más:

```bash
$ ax affected --target test
docs:test
synapse-gateway:test
```

Cada tarea afectada indica por qué, como `files`, `lockfile`, `manifest`, `dependency` o
`upstream`, en `ax affected --json`.

## Pruébalo {#try-it}

Instala el binario con Cargo y ejecútalo desde cualquier punto del repositorio:

```bash
cargo install --locked --git https://github.com/sustentabilitas/axonal axonal
ax graph
ax run fmt lint test --affected
```

[Compilar con axonal](/docs/contributing/axonal/) explica en detalle cómo funciona, la
configuración de este repositorio y cada comando y opción.

## Qué viene después {#whats-next}

axonal es experimental: los comandos, la configuración y el formato de la caché pueden cambiar
entre versiones, y los comandos `cargo` y `npm` siguen siendo la referencia de lo que una pull
request debe superar. Lo siguiente en la lista:

- **La CI de este repositorio,** que ejecutará sus comprobaciones con `ax`.
- **Una caché remota** en un bucket que controlas tú (primero Google Cloud Storage, después
  S3), para que los runners de CI y los portátiles compartan resultados.
- **La poda.** El conjunto de afectadas es una cota superior, y la poda lo reducirá, primero con
  pruebas deterministas y después con juicios tipados de TypeSafe System One (Jev), con umbrales
  de confianza y un modo sombra que mide la seguridad antes de saltarse nada.
- **Un migrador de Nx** que convierte un workspace de Nx en un único `axonal.toml`.

El código está en [GitHub](https://github.com/sustentabilitas/axonal), con licencia MPL-2.0.
Pruébalo en tu propio workspace y cuéntanos en qué se equivoca.
