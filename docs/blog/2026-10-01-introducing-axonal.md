---
slug: introducing-axonal
title: "Introducing axonal: one task runner for Cargo and pnpm"
authors: [rajwilkhu]
tags: [tooling]
date: 2026-10-01T10:00
description: axonal is an experimental monorepo task runner, written in Rust, that infers projects from Cargo and pnpm manifests, caches every task and runs only what a change can affect. Synapse now uses it.
---

Synapse is a Cargo workspace of five crates with an npm documentation site beside it, and
until now every pull request ran every Rust check: formatting, clippy and the tests of all
five crates, and five feature builds, whether the change touched a crate or fixed a typo in
the docs. [axonal](https://github.com/sustentabilitas/axonal) is the task runner we built
to stop that. Its command is `ax`, and as of today this repository has an `axonal.toml`.

axonal is experimental. This post explains what it does, how it decides what to run, and what
comes next.

<!-- truncate -->

## Why another task runner

Most monorepo task runners grew up around JavaScript, and Cargo is an add-on for them. We
wanted three things from one tool: mixed Cargo and pnpm repositories as first-class citizens,
configuration that fits in one small file, and a model of the workspace precise enough to say
which checks a change can break. axonal is a single Rust binary that does the first two today
and lays the groundwork for the third.

## It reads the manifests you already have

axonal finds projects the way your build tools do. Every Cargo workspace member is a project,
found with `cargo metadata`, and gets `build`, `test`, `lint` (clippy) and `fmt` targets.
Every pnpm workspace package is a project whose `package.json` scripts are its targets.
Dependencies between them become edges, and so do TypeScript imports between projects.
Anything else is declared in `axonal.toml` with plain shell commands. Synapse's docs site uses
npm rather than pnpm, so it's declared there:

```toml
[projects.docs.targets.test]
command = "npm test"
```

`ax graph` builds the whole graph for this repository in about 30 milliseconds.

## Every task has a key

Before it runs a task, axonal hashes everything that can change its result: the command, the
project's input files, the lockfile and toolchain files, the matching files of every project
it depends on, the listed environment variables and the versions of `rustc`, `node` and
`pnpm`. If the local cache already holds a successful run for that key, axonal replays the logs
and restores the outputs instead of running it. Run `ax run fmt lint test` twice, and the
second run reports nothing but cache hits.

Hashing dependencies' files matters more in Cargo than it first seems. `cargo test -p
synapse-gateway` has no ordering dependency on `synapse-a2a`, because Cargo builds it as part
of the test run, but a change to `synapse-a2a` must still re-run the gateway's tests. axonal
includes the dependency's sources in the gateway's key, so it does.

## Only what a change can affect

`ax run test --affected` compares your branch with the merge-base on `main`, maps each
changed file to its project and the targets whose inputs match it, and adds every project
downstream. A changed `Cargo.lock` is parsed in both versions and diffed, so updating one
dependency affects only the projects that actually use it.

The set has one rule it must never break: it is an upper bound. Running a check that didn't
need to run wastes a minute; skipping one that did lets a broken change merge. A property test
in axonal edits random files in random workspaces and checks that every task whose cache key
changed is in the affected set.

This repository shows why a precise model matters. The gateway's `docs_examples` test parses
the configuration examples on this site, so a docs-only change can break a Rust test. One
line in `axonal.toml` tells axonal the test reads the docs:

```toml
[projects."crates/synapse-gateway".targets.test]
inputs = ["**/*", "{workspace}/docs/docs/**", "{workspace}/docs/i18n/**"]
```

Now editing a page affects the docs site's tests and the gateway's tests, and nothing else:

```bash
$ ax affected --target test
docs:test
synapse-gateway:test
```

Every affected task records why, as `files`, `lockfile`, `manifest`, `dependency` or
`upstream`, in `ax affected --json`.

## Try it

Install the binary with Cargo and run it from anywhere in the repository:

```bash
cargo install --locked --git https://github.com/sustentabilitas/axonal axonal
ax graph
ax run fmt lint test --affected
```

[Building with axonal](/docs/contributing/axonal/) covers how it works in detail, this
repository's configuration and every command and option.

## What's next

axonal is experimental: commands, configuration and the cache format can change between
releases, and the plain `cargo` and `npm` commands remain the reference for what a pull request
must pass. Next on the list:

- **CI for this repository,** which will run its checks through `ax`.
- **A remote cache** in a bucket you own (Google Cloud Storage first, then S3), so CI runners
  and laptops share results.
- **Pruning.** The affected set is an upper bound, and pruning will shrink it, first with
  deterministic proofs, then with typed judgements from TypeSafe System One (Jev), behind
  confidence thresholds and a shadow mode that measures safety before anything is skipped.
- **An Nx migrator** that turns an Nx workspace into one `axonal.toml`.

The code is on [GitHub](https://github.com/sustentabilitas/axonal), under MPL-2.0. Try it on
your own workspace, and tell us what it gets wrong.
