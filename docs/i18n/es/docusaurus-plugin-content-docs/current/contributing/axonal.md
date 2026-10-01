---
sidebar_position: 2
title: Compilar con axonal
description: Cómo axonal, el ejecutor de tareas experimental de este repositorio, modela el workspace, cachea las tareas y averigua qué afecta un cambio, y cómo usar su comando ax.
---

[axonal](https://github.com/sustentabilitas/axonal) es un ejecutor de tareas para monorepos con
workspaces de Cargo y pnpm, y su comando es `ax`. Este repositorio contiene cinco crates de
Cargo y un sitio de documentación con npm. axonal ejecuta las comprobaciones de todos ellos en
orden de dependencias, se salta las que no han cambiado de entradas desde la última vez que
pasaron y, en una rama, ejecuta solo las comprobaciones a las que pueden afectar los cambios de
esa rama.

:::warning Experimental
axonal está en desarrollo activo. Sus comandos, su configuración y el formato de su caché
pueden cambiar entre versiones. Los comandos `cargo` y `npm` de [Contribuir](contributing.md)
siempre funcionan y siguen siendo la referencia de lo que una pull request debe superar.
:::

## Instalación {#install}

axonal es un único binario de Rust. Instálalo desde el repositorio con Cargo (Rust 1.96 o
posterior):

```bash
cargo install --locked --git https://github.com/sustentabilitas/axonal axonal
ax --version
```

## Inicio rápido {#quick-start}

Desde cualquier punto del repositorio:

```bash
# Una vez, para que puedan ejecutarse los targets de la documentación
(cd docs && npm ci)

# Proyectos, sus dependencias y sus targets
ax graph

# Formato, clippy y tests de cada crate, más los tests del sitio de documentación
ax run fmt lint test

# Lo mismo otra vez: ahora cada tarea es un acierto de caché y termina al instante
ax run fmt lint test

# Solo las tareas a las que pueden afectar los cambios de tu rama
ax run fmt lint test --affected
```

## Cómo funciona {#how-it-works}

### Proyectos y targets {#projects-and-targets}

axonal lee los manifiestos que el repositorio ya tiene, así que la mayoría de los proyectos no
necesitan configuración:

- **Cargo.** Cada miembro del workspace de Cargo es un proyecto, que se encuentra con
  `cargo metadata --no-deps`. Una dependencia por ruta sobre otro miembro es una arista de
  dependencia; una que solo es de desarrollo cuenta para la caché y para la detección de
  afectados, pero no ordena tareas.
- **pnpm.** Cada paquete que incluye `pnpm-workspace.yaml` es un proyecto, las dependencias
  `workspace:` son aristas y cada script de `package.json` es un target que se ejecuta con
  `pnpm run <script>`. Los imports de TypeScript entre proyectos también son aristas.
- **Proyectos explícitos.** Todo lo demás, como el sitio de documentación de este repositorio,
  que usa npm, se declara en `axonal.toml` con comandos de shell normales.

Cada crate de Cargo recibe cuatro targets:

| Target | Comando |
|---|---|
| `build` | `cargo build -p <crate>` |
| `test` | `cargo test -p <crate>` |
| `lint` | `cargo clippy -p <crate> --all-targets -- -D warnings` |
| `fmt` | `cargo fmt -p <crate> -- --check` |

`ax graph` muestra lo que ha encontrado axonal. En este repositorio:

```text
docs (docs)
  deps: -
  targets: build, i18n, test, typecheck
synapse-a2a (crates/synapse-a2a)
  deps: -
  targets: build, fmt, lint, test
synapse-context (crates/synapse-context)
  deps: -
  targets: build, fmt, lint, test
synapse-gateway (crates/synapse-gateway)
  deps: synapse-a2a
  targets: build, build-cloud, build-lean, build-postgres, build-pubsub, build-sns, fmt, lint, test
synapse-mcp (crates/synapse-mcp)
  deps: synapse-context
  targets: build, fmt, lint, test
synapse-proxy (crates/synapse-proxy)
  deps: synapse-context
  targets: build, fmt, lint, test
```

### Ejecutar tareas {#running-tasks}

Una tarea es un target de un proyecto, escrita `proyecto:target`, como `synapse-gateway:test`.
`ax run fmt lint` ejecuta `fmt` y `lint` en cada proyecto que los tiene, o solo en los
proyectos que indiques con `-p`. El `depends_on` de un target ordena las tareas:
`depends_on = ["^build"]` ejecuta primero `build` en las dependencias del proyecto. Las tareas
independientes se ejecutan en paralelo, hasta una por núcleo de CPU, y cada línea de salida
lleva como prefijo su tarea.

Cada tarea se ejecuta con `sh -c` en el directorio de su proyecto. Si una falla, axonal no
inicia tareas nuevas, deja terminar las que están en marcha y sale con `1`; con `--continue`
sigue ejecutando cada tarea que no dependa del fallo. Ctrl-C detiene todas las tareas y da a
cada una cinco segundos para salir; un segundo Ctrl-C las termina al instante.

### La caché {#the-cache}

Antes de ejecutar una tarea, axonal calcula una clave a partir de todo lo que puede cambiar su
resultado:

- el comando y la configuración del target;
- el contenido de los archivos de entrada del proyecto (`inputs`; por defecto, todos los
  archivos del proyecto que `.gitignore` no excluye);
- los archivos que siempre son relevantes: el manifiesto del proyecto y, para los proyectos de
  Cargo, `Cargo.lock`, el `Cargo.toml` raíz, `.cargo/config*` y `rust-toolchain*`;
- los archivos de sus dependencias que coinciden con los mismos `inputs`, de modo que un cambio
  en `synapse-a2a` vuelve a ejecutar los tests del gateway aunque
  `cargo test -p synapse-gateway` no tenga `depends_on`;
- los valores de las variables de entorno que lista `env`;
- las versiones de `rustc`, `node` y `pnpm`, y el sistema operativo y la arquitectura de CPU.

Si la caché local, en `.axonal/cache`, contiene un resultado correcto para esa clave, axonal
reproduce los logs de la tarea y restaura sus `outputs` declarados en lugar de ejecutarla. Solo
se guardan las tareas que terminan bien. Los targets de Cargo guardan solo su resultado y sus
logs, porque Cargo mantiene sus propios artefactos de compilación en `target/`. La caché
desaloja las entradas usadas hace más tiempo cuando supera los 10 GB.

### Tareas afectadas {#affected-tasks}

`ax affected` y `ax run --affected` comparan tu trabajo con la revisión base: por defecto, el
merge-base de `HEAD` y `main` (u `origin/main` si no hay una `main` local). Los archivos sin
confirmar y sin seguimiento también cuentan. Después:

1. Cada archivo cambiado pertenece al proyecto con la raíz más profunda que lo contiene, y
   afecta a los targets cuyos `inputs` coinciden con él.
2. Un target que lista archivos fuera de su proyecto mediante globs `{workspace}/` se ve
   afectado por los cambios en esos archivos.
3. Los proyectos que dependen de un proyecto cambiado, directa o transitivamente, también se
   ven afectados, igual que cada tarea que depende de una tarea afectada mediante
   `depends_on`. Una tarea afectada cuya clave no ha cambiado, como el `fmt` de un
   dependiente, sigue siendo un acierto de caché.
4. Un `Cargo.lock` o `pnpm-lock.yaml` cambiado se analiza en sus dos versiones y se compara,
   y solo se ven afectados los proyectos cuyos paquetes externos han cambiado.
5. Un cambio en un archivo que incluyen todas las claves, como el `Cargo.toml` raíz o
   `rust-toolchain.toml`, afecta a todos los proyectos que lo usan, y un cambio en
   `axonal.toml` lo afecta todo.

Por ejemplo, editar una página de esta documentación afecta a los tests del sitio de
documentación y, como `docs_examples` analiza los ejemplos de configuración de la
documentación, a los tests del gateway, pero no a clippy ni a ningún otro crate:

```bash
$ ax affected --target test
docs:test
synapse-gateway:test
$ ax affected --target lint
$
```

`ax affected --json` también indica por qué está afectada cada tarea: `files`, `lockfile`,
`manifest`, `dependency` o `upstream`.

## Configuración de este repositorio {#configuration-in-this-repository}

El `axonal.toml` del repositorio añade lo que la inferencia no puede ver: los archivos de CI de
los que dependen todas las tareas, la documentación que leen los tests del gateway, las
compilaciones con features del gateway y el sitio de documentación.

```toml title="axonal.toml"
[workspace]
default_branch = "main"
# CI runs every task through ax, so a change to how it does that re-runs every task.
inputs = [".github/workflows/ci.yml", ".github/actions/setup-ax/**"]

# docs_examples parses the config examples in the docs, so docs changes re-run the tests.
[projects."crates/synapse-gateway".targets.test]
inputs = ["**/*", "{workspace}/docs/docs/**", "{workspace}/docs/i18n/**"]

[projects."crates/synapse-gateway".targets.build-postgres]
command = "cargo build -p synapse-gateway --features ledger-postgres"

[projects."crates/synapse-gateway".targets.build-pubsub]
command = "cargo build -p synapse-gateway --features ledger-pubsub"

[projects."crates/synapse-gateway".targets.build-sns]
command = "cargo build -p synapse-gateway --features ledger-sns"

[projects."crates/synapse-gateway".targets.build-cloud]
command = "cargo build -p synapse-gateway --features 'ledger-pubsub ledger-sns'"

[projects."crates/synapse-gateway".targets.build-lean]
command = "cargo build -p synapse-gateway --no-default-features --lib"

# The docs site uses npm, not pnpm, so it's declared here. Run `npm ci` in docs/ first.
[projects.docs.targets.test]
command = "npm test"

[projects.docs.targets.i18n]
command = "npm run check:i18n"

[projects.docs.targets.typecheck]
command = "npm run typecheck"

[projects.docs.targets.build]
command = "npm run build"
inputs = ["**/*", "{workspace}/.readthedocs.yaml"]
```

Las comprobaciones de [Antes de enviar](contributing.md#before-you-submit) corresponden a estos
targets:

| Comprobación | Con axonal |
|---|---|
| `cargo fmt --all --check` | `ax run fmt` |
| `cargo clippy --all-targets -- -D warnings` | `ax run lint` |
| `cargo test` | `ax run test -p synapse-gateway -p synapse-proxy -p synapse-mcp -p synapse-context -p synapse-a2a` |
| Las compilaciones con features | `ax run build-postgres build-pubsub build-sns build-cloud build-lean` |
| Las comprobaciones de la documentación | `ax run test i18n typecheck build -p docs` |

`ax run test` sin más ejecuta a la vez los tests de cada crate y los del sitio de
documentación.

## En la CI {#in-ci}

El workflow de CI ejecuta estas comprobaciones con `ax`, en los mismos jobs paralelos que antes:
uno para el formato, clippy y los tests, uno por cada compilación con features, uno para las
compilaciones por defecto y ligera de los crates, y uno para el sitio de documentación. Cada job
instala `ax` en un commit fijado de axonal y restaura su caché de tareas de ejecuciones
anteriores.

- **En una pull request,** cada job ejecuta `ax run <targets> --affected`, así que un cambio
  ejecuta solo las tareas a las que puede afectar, y un job sin nada afectado no ejecuta
  ninguna tarea. Un cambio solo en la documentación, por ejemplo, ejecuta las comprobaciones de
  la documentación y los tests del gateway.
- **En `main`,** cada job ejecuta todas las tareas, y la caché de tareas se salta las que no han
  cambiado de entradas desde la última vez que pasaron.

Un cambio en el workflow, en la acción que instala `ax` o en `axonal.toml` afecta a todas las
tareas, así que lo ejecuta todo.

## Referencia de comandos {#command-reference}

| Comando | Qué hace |
|---|---|
| `ax run <target>…` | Ejecuta los targets en orden de dependencias, usando la caché. |
| `ax affected` | Lista los proyectos afectados desde la base; con `--target <nombre>` lista en su lugar las tareas afectadas de ese target. |
| `ax graph` | Muestra los proyectos, sus dependencias y sus targets; `--json` o `--dot` para otros formatos. |
| `ax cache stats` | Muestra el número y el tamaño total de las entradas de la caché local. |
| `ax cache clean` | Borra la caché local. |
| `ax init` | Escribe un `axonal.toml` inicial con los proyectos descubiertos; `--force` sobrescribe uno existente. |

Opciones de `ax run`:

| Opción | Efecto |
|---|---|
| `-p, --project <nombre>` | Ejecuta los targets solo en estos proyectos (repetible); las tareas de las que dependen se siguen ejecutando. |
| `--affected` | Ejecuta solo las tareas afectadas. |
| `--base <rev>` | La revisión base para `--affected` (`ax affected` también la acepta). |
| `--head <rev>` | La revisión final. `ax run` sigue incluyendo los cambios del árbol de trabajo, porque las tareas se ejecutan sobre él. |
| `--parallel <n>` | El número máximo de tareas simultáneas (por defecto, el número de núcleos de CPU). |
| `--continue` | Sigue ejecutando tareas independientes después de un fallo. |
| `--no-cache` | Ejecuta todas las tareas y no guarda nada. |
| `--json` | Escribe un informe JSON de la ejecución en la salida estándar y la salida de las tareas en la salida de error. |

Todos los comandos aceptan `--cwd <dir>` para ejecutarse como si se hubieran iniciado en otro
directorio. El código de salida es `0` si todo va bien, `1` si una tarea ha fallado, `2` ante
un error de configuración o de uso y `130` si se ha interrumpido la ejecución.

## Referencia de configuración {#configuration-reference}

`axonal.toml` está en la raíz del repositorio. Hoy se admiten estas claves:

| Clave | Significado |
|---|---|
| `[workspace] default_branch` | La rama con la que compara `--affected` (por defecto `main`). |
| `[workspace] inputs` | Globs, relativos a la raíz, de archivos que entran en la clave de cada tarea. |
| `[targets.<nombre>]` | Valores por defecto del target `<nombre>` de cada proyecto. |
| `[projects."<ruta>"]` | Un proyecto en `<ruta>`: declara un proyecto sin manifiesto o modifica uno inferido. Acepta `name`, `deps` (aristas de dependencia adicionales) y `targets`. |
| `command` | El comando de shell que ejecuta un target. |
| `depends_on` | Targets que se ejecutan antes: `build` en el mismo proyecto, `^build` en sus dependencias. |
| `inputs` | Globs, relativos al proyecto, de los archivos que lee la tarea (por defecto `**/*`); `{workspace}/` hace que un glob sea relativo a la raíz. |
| `outputs` | Globs de los archivos que escribe la tarea, que se guardan en la caché y se restauran en un acierto. |
| `env` | Variables de entorno cuyos valores forman parte de la clave. |
| `persistent` | `true` para tareas de larga duración, como servidores de desarrollo: nunca se cachean y `--affected` nunca las ejecuta. |
| `deps_usage` | `none` deja fuera de la clave los archivos de las dependencias (el valor por defecto de `fmt`). |
| `[cache] local_max_size` | El tamaño máximo de la caché local, como `"500MB"` (por defecto `"10GB"`). |

## Limitaciones {#limitations}

- **Solo caché local.** Cada máquina tiene su propia caché. Están previstas una caché remota
  compartida y la poda (descartar tareas afectadas de las que se puede demostrar que no hace
  falta ejecutarlas).
- **La detección de afectados necesita el historial de git.** Un clon superficial puede no
  contener el merge-base; descarga el historial completo (en GitHub Actions,
  `fetch-depth: 0`) o pasa `--base`.
- **Entradas literales ignoradas por git.** Una entrada indicada por su nombre, como
  `.env.local`, entra en la clave aunque git la ignore, pero git no informa de sus cambios, así
  que nunca hace que una tarea quede afectada.
- **Los workspaces de npm no se infieren,** y por eso el sitio de documentación se declara de
  forma explícita. Un proyecto explícito incluye en su clave los archivos siempre relevantes
  tanto de Cargo como de pnpm, así que un cambio en `Cargo.lock` también vuelve a ejecutar las
  tareas de la documentación.
