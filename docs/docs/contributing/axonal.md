---
sidebar_position: 2
title: Building with axonal
description: How axonal, the experimental task runner for this repository, models the workspace, caches tasks and works out what a change affects, and how to use its ax command.
---

[axonal](https://github.com/sustentabilitas/axonal) is a monorepo task runner for Cargo and
pnpm workspaces, and its command is `ax`. This repository holds five Cargo crates and an npm
documentation site. axonal runs the checks for all of them in dependency order, skips any check
whose inputs haven't changed since it last passed, and, on a branch, runs only the checks that
the branch's changes can affect.

:::warning Experimental
axonal is under active development. Its commands, configuration and cache format may change
between releases. The plain `cargo` and `npm` commands in [Contributing](contributing.md)
always work, and they remain the reference for what a pull request must pass.
:::

## Install

axonal is a single Rust binary. Install it from the repository with Cargo (Rust 1.96 or later):

```bash
cargo install --locked --git https://github.com/sustentabilitas/axonal axonal
ax --version
```

## Quick start

From anywhere inside the repository:

```bash
# Once, so the docs targets can run
(cd docs && npm ci)

# Projects, their dependencies and their targets
ax graph

# Format check, clippy and tests for every crate, plus the docs site's tests
ax run fmt lint test

# The same again: every task is now a cache hit and finishes at once
ax run fmt lint test

# Only the tasks that your branch's changes can affect
ax run fmt lint test --affected
```

## How it works

### Projects and targets

axonal reads the manifests the repository already has, so most projects need no configuration:

- **Cargo.** Every member of the Cargo workspace is a project, found with
  `cargo metadata --no-deps`. A path dependency on another member is a dependency edge; a
  dev-only one counts for caching and affected detection but doesn't order tasks.
- **pnpm.** Every package matched by `pnpm-workspace.yaml` is a project, `workspace:`
  dependencies are edges, and each `package.json` script is a target run with
  `pnpm run <script>`. TypeScript imports between projects are edges too.
- **Explicit projects.** Anything else, such as this repository's npm-based docs site, is
  declared in `axonal.toml` with plain shell commands.

Each Cargo crate gets four targets:

| Target | Command |
|---|---|
| `build` | `cargo build -p <crate>` |
| `test` | `cargo test -p <crate>` |
| `lint` | `cargo clippy -p <crate> --all-targets -- -D warnings` |
| `fmt` | `cargo fmt -p <crate> -- --check` |

`ax graph` prints what axonal found. In this repository:

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

### Running tasks

A task is one target of one project, written `project:target`, such as `synapse-gateway:test`.
`ax run fmt lint` runs `fmt` and `lint` in every project that has them, or only in the projects
you name with `-p`. A target's `depends_on` orders tasks: `depends_on = ["^build"]` runs
`build` in a project's dependencies first. Independent tasks run in parallel, up to one per CPU
core, and each line of output is prefixed with its task.

Each task runs through `sh -c` in its project's directory. On failure, axonal starts no new
tasks, lets the running ones finish and exits with `1`; with `--continue` it keeps running
every task that doesn't depend on the failure. Ctrl-C stops every task, giving each five
seconds to exit; a second Ctrl-C ends them at once.

### The cache

Before it runs a task, axonal computes a key from everything that can change the task's
result:

- the command and the target's configuration;
- the content of the project's input files (`inputs`, by default every file in the project
  that `.gitignore` doesn't exclude);
- the files that are always relevant: the project's manifest and, for Cargo projects,
  `Cargo.lock`, the root `Cargo.toml`, `.cargo/config*` and `rust-toolchain*`;
- the files of its dependencies that match the same `inputs`, so a change in `synapse-a2a`
  re-runs the gateway's tests even though `cargo test -p synapse-gateway` has no `depends_on`;
- the values of the environment variables listed in `env`;
- the versions of `rustc`, `node` and `pnpm`, and the operating system and CPU architecture.

If the local cache, in `.axonal/cache`, holds a successful result for the key, axonal replays
the task's logs and restores its declared `outputs` instead of running it. Only successful
tasks are stored. Cargo targets store only their result and logs, because Cargo keeps its own
build artefacts in `target/`. The cache evicts the least recently used entries beyond 10 GB.

### Affected tasks

`ax affected` and `ax run --affected` compare your work with the base revision: by default
the merge-base of `HEAD` and `main` (or `origin/main` if there is no local `main`).
Uncommitted and untracked files count too. Then:

1. Each changed file belongs to the project with the deepest root containing it, and affects
   the targets whose `inputs` match it.
2. A target that lists files outside its project through `{workspace}/` globs is affected by
   changes to those files.
3. The projects that depend on a changed project, directly or transitively, are affected
   too, as is every task that depends on an affected task through `depends_on`. An affected
   task whose key didn't change, such as a dependent's `fmt`, is still a cache hit.
4. A changed `Cargo.lock` or `pnpm-lock.yaml` is parsed in both versions and diffed, and only
   the projects whose external packages changed are affected.
5. A change to a file every key includes, such as the root `Cargo.toml` or
   `rust-toolchain.toml`, affects every project that hashes it, and a change to `axonal.toml`
   affects everything.

For example, editing a page of these docs affects the docs site's tests and, because
`docs_examples` parses the configuration examples in the docs, the gateway's tests, but not
clippy or any other crate:

```bash
$ ax affected --target test
docs:test
synapse-gateway:test
$ ax affected --target lint
$
```

`ax affected --json` also records why each task is affected: `files`, `lockfile`, `manifest`,
`dependency` or `upstream`.

## Configuration in this repository

The repository's `axonal.toml` adds what inference can't see: the docs that the gateway's
tests read, the gateway's feature builds from CI, and the docs site.

```toml title="axonal.toml"
[workspace]
default_branch = "main"

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
```

The checks from [Before you submit](contributing.md#before-you-submit) map onto these
targets:

| Check | With axonal |
|---|---|
| `cargo fmt --all --check` | `ax run fmt` |
| `cargo clippy --all-targets -- -D warnings` | `ax run lint` |
| `cargo test` | `ax run test -p synapse-gateway -p synapse-proxy -p synapse-mcp -p synapse-context -p synapse-a2a` |
| The feature builds | `ax run build-postgres build-pubsub build-sns build-cloud build-lean` |
| The docs checks | `ax run test i18n typecheck build -p docs` |

`ax run test` on its own runs the tests of every crate and the docs site together.

## Command reference

| Command | What it does |
|---|---|
| `ax run <target>…` | Runs the targets in dependency order, using the cache. |
| `ax affected` | Lists the projects affected since the base; `--target <name>` lists that target's affected tasks instead. |
| `ax graph` | Prints the projects, their dependencies and their targets; `--json` or `--dot` for other formats. |
| `ax cache stats` | Shows the number and total size of local cache entries. |
| `ax cache clean` | Deletes the local cache. |
| `ax init` | Writes a starter `axonal.toml` listing the discovered projects; `--force` overwrites one. |

Options for `ax run`:

| Option | Effect |
|---|---|
| `-p, --project <name>` | Runs the targets only in these projects (repeatable); tasks they depend on still run. |
| `--affected` | Runs only the affected tasks. |
| `--base <rev>` | The base revision for `--affected` (`ax affected` takes it too). |
| `--head <rev>` | The head revision. `ax run` still includes the working tree's changes, because tasks run against it. |
| `--parallel <n>` | The maximum number of concurrent tasks (default: the number of CPU cores). |
| `--continue` | Keeps running independent tasks after a failure. |
| `--no-cache` | Runs every task and stores nothing. |
| `--json` | Prints a JSON run report on standard output and task output on standard error. |

Every command takes `--cwd <dir>` to run as if started in another directory. The exit code is
`0` on success, `1` if a task failed, `2` for a configuration or usage error and `130` if the
run was interrupted.

## Configuration reference

`axonal.toml` lives at the repository root. These keys are supported today:

| Key | Meaning |
|---|---|
| `[workspace] default_branch` | The branch `--affected` compares against (default `main`). |
| `[workspace] inputs` | Globs, relative to the root, of files hashed into every task's key. |
| `[targets.<name>]` | Defaults for every project's `<name>` target. |
| `[projects."<path>"]` | A project at `<path>`: declares a project without a manifest, or overrides an inferred one. Takes `name`, `deps` (extra dependency edges) and `targets`. |
| `command` | The shell command a target runs. |
| `depends_on` | Targets to run first: `build` in the same project, `^build` in its dependencies. |
| `inputs` | Globs, relative to the project, of the files the task reads (default `**/*`); `{workspace}/` makes a glob relative to the root. |
| `outputs` | Globs of the files the task writes, stored in the cache and restored on a hit. |
| `env` | Environment variables whose values are part of the key. |
| `persistent` | `true` for long-running tasks such as dev servers: never cached and never run by `--affected`. |
| `deps_usage` | `none` leaves dependencies' files out of the key (the default for `fmt`). |
| `[cache] local_max_size` | The local cache's size limit, such as `"500MB"` (default `"10GB"`). |

## Limitations

- **Local cache only.** Each machine has its own cache. A shared remote cache, and pruning
  (dropping affected tasks that can be shown not to need running), are planned.
- **Affected detection needs git history.** A shallow clone may not contain the merge-base;
  fetch the full history (in GitHub Actions, `fetch-depth: 0`) or pass `--base`.
- **Gitignored literal inputs.** An input listed by name, such as `.env.local`, is hashed even
  when gitignored, but git doesn't report its changes, so it never makes a task affected.
- **npm workspaces aren't inferred,** which is why the docs site is declared explicitly. An
  explicit project hashes the always-relevant files of both Cargo and pnpm, so a change to
  `Cargo.lock` also re-runs the docs tasks.
