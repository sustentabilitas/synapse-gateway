---
sidebar_position: 1
title: Contributing
description: How to build and test the Synapse workspace, prepare a pull request, sign off your commits and work on this documentation site.
---

Synapse is an open-source, MPL-2.0-licensed project, and issues, bug reports, feature requests
and pull requests are welcome. Before you begin, read the
[Code of Conduct](https://github.com/sustentabilitas/synapse-gateway/blob/main/CODE_OF_CONDUCT.md)
and the [security policy](../operating/security.md). By taking part you agree to abide by the
Code of Conduct.

## Getting started

### Prerequisites

- The current stable [Rust toolchain](https://rustup.rs/), with `rustfmt` and `clippy` (both
  ship with `rustup`). The repository doesn't pin a toolchain; CI uses the latest stable.
- Optional: Docker, to build the gateway and proxy images.
- Optional: Node.js 22, to work on this documentation site.

### Clone and build

```bash
git clone https://github.com/sustentabilitas/synapse-gateway.git
cd synapse-gateway

# Build every crate in the workspace (gateway default features: server + ledger-sqlite)
cargo build

# Run the test suite
cargo test
```

The workspace has five crates under `crates/`; see
[Workspace crates](../internals/workspace-crates.md) for what each one does.

### Feature matrix

The gateway has optional ledger backends and a lean library build. When your change touches a
ledger backend or the embeddable library surface, build the variants it affects:

| Command | What it enables |
|---|---|
| `cargo build -p synapse-gateway` | Default features (`server` + `ledger-sqlite`) |
| `cargo build -p synapse-gateway --features ledger-postgres` | PostgreSQL cost-ledger sink |
| `cargo build -p synapse-gateway --features ledger-pubsub` | Google Cloud Pub/Sub ledger sink |
| `cargo build -p synapse-gateway --features ledger-sns` | AWS SNS ledger sink |
| `cargo build -p synapse-gateway --features "ledger-pubsub ledger-sns"` | Both cloud ledger sinks together |
| `cargo build -p synapse-gateway --no-default-features --lib` | Lean embeddable core (no HTTP server, no ledger) |

## Before you submit

Run these checks locally before you open a pull request. CI runs each of them, and a failing
check blocks the merge.

```bash
# 1. Formatting (must produce no diff)
cargo fmt --all --check

# 2. Lints, with warnings treated as errors
cargo clippy --all-targets -- -D warnings

# 3. Tests, with default features
cargo test

# 4. The feature variants your change touches
cargo build -p synapse-gateway --features ledger-postgres
cargo build -p synapse-gateway --features ledger-pubsub
cargo build -p synapse-gateway --features ledger-sns
cargo build -p synapse-gateway --features "ledger-pubsub ledger-sns"
cargo build -p synapse-gateway --no-default-features --lib
```

Also:

- **Update the changelog.** Each crate has its own `crates/<crate>/CHANGELOG.md`. For the
  gateway, add a line under `## [Unreleased]`, in the `Added`, `Changed`, `Fixed`, `Removed` or
  `Security` group ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)); the release
  workflow turns that section into the new version. For the other crates, the release workflow
  adds the new version's heading at the top of the file, so add your line there when you release
  (see [Releasing](releasing.md)).
- **Update the documentation.** If your change affects the public API, configuration, HTTP
  endpoints, metrics or behaviour, update the pages on this site (see
  [Working on the docs](#working-on-the-docs)) and the rustdoc comments.

## Development workflow

Non-trivial contributions, such as new features, significant refactors, new ledger backends or
changes to the public library API, follow a **spec, plan, implement** flow:

1. **Write a spec.** Describe *what* and *why*: the problem, the proposed behaviour, edge cases
   and acceptance criteria. Keep it short.
2. **Write a plan.** Break the work into small, reviewable steps that reference the spec.
3. **Implement with TDD.** Write the failing test first, in `tests/` or a `#[cfg(test)]`
   module next to the code, then the smallest implementation that passes, then refactor.
   Commit the test separately from the implementation where that helps review.
4. **Open a pull request** that links to or summarises the spec and plan, so reviewers have
   the full context.

Specs and plans are working documents: don't commit them under `docs/`, which holds this site.
Small bug fixes and documentation improvements don't need a spec; use your judgement.

## Commit messages

- Write an **imperative, present-tense subject line**, such as `add Pub/Sub ledger sink`, not
  `added` or `adding`.
- Keep the subject under **72 characters**.
- Use a [conventional-commit](https://www.conventionalcommits.org/) prefix, scoped to the
  crate you change where that helps, for example `fix(synapse-proxy): ...`:

  | Prefix | Use for |
  |---|---|
  | `feat:` | New feature or behaviour |
  | `fix:` | Bug fix |
  | `docs:` | Documentation changes only |
  | `refactor:` | Code restructuring without behaviour change |
  | `test:` | Adding or updating tests |
  | `chore:` | Maintenance, dependency updates, tooling |
  | `perf:` | Performance improvements |
  | `ci:` | CI/CD pipeline changes |

- Optionally add a body, after a blank line, that explains *why* you made the change.
- Reference related issues or pull requests at the bottom, for example `Closes #42`.

```text
feat(synapse-gateway): add AWS SNS ledger sink

Adds a fan-out sink that publishes cost-ledger events to an SNS topic.
Gated behind the `ledger-sns` feature flag.

Closes #17
Signed-off-by: Your Name <your@email.com>
```

## Developer Certificate of Origin

**Every commit must carry a `Signed-off-by` trailer.** By signing off, you certify that you
have the right to submit the contribution under the project's MPL-2.0 licence, as defined by
the [Developer Certificate of Origin](https://developercertificate.org/).

Add the sign-off with the `-s` flag:

```bash
git commit -s -m "feat: your change description"
```

This appends a line like the following, with your real name and a working email address:

```text
Signed-off-by: Your Name <your@email.com>
```

:::warning
Pull requests that contain unsigned commits are not merged. If you forgot to sign off earlier
commits, amend them:

```bash
# The most recent commit
git commit --amend -s --no-edit

# Every commit on the branch
git rebase --signoff HEAD~<N>
```
:::

## Pull requests

1. **Fork** the repository and create a feature branch from `main`.
2. Follow the [development workflow](#development-workflow) and the
   [commit message guidelines](#commit-messages).
3. Make sure **every CI check passes** before you request a review.
4. Open a pull request with:
   - a clear, conventional-commit-style title;
   - a description of *what* changed and *why*;
   - links to the spec and plan for non-trivial changes;
   - `Closes #<issue>` if it applies.
5. Address review feedback promptly. Merging needs one approving review from a maintainer.
6. Maintainers may squash or rebase on merge to keep the history clean.

## Working on the docs

This site is a [Docusaurus](https://docusaurus.io/) project in the `docs/` directory, in
English and Spanish. To run it locally with live reload:

```bash
cd docs && npm ci && npm start
```

`npm start` serves one locale at a time. To preview the Spanish site:

```bash
npm start -- --locale es
```

Before you open a pull request that touches `docs/`, run the same checks as CI:

```bash
npm test              # unit tests for the site's scripts
npm run check:i18n    # every English page has a Spanish twin
npm run typecheck
npm run build         # builds both locales; fails on broken links and anchors
```

Pages are Markdown (`.md`) files under `docs/docs/`, with `sidebar_position`, `title` and
`description` front matter, and relative links that include the `.md` extension. Two rules keep
the site honest:

- **Spanish parity.** A pull request that adds or changes an English page updates its Spanish
  twin under `docs/i18n/es/docusaurus-plugin-content-docs/current/`, at the same relative path.
  CI runs `npm run check:i18n`, which fails when a page exists in one language and not the
  other.
- **Titled config examples are tested.** A code block fenced with a title that ends in
  `routes.toml`, `pricing.toml`, `guardrails.toml` or `ai_task_types.toml`, such as
  ```` ```toml title="config/routes.toml" ````, must be a complete, valid file. The gateway's own
  parsers load every one of them in both languages when you run
  `cargo test -p synapse-gateway --test docs_examples`, which is part of `cargo test`. Fence
  partial fragments with a plain ```` ```toml ```` and no title.

`docs/superpowers/` is gitignored and local-only: keep working notes there, and never commit
anything under it.

## License

By submitting a contribution you agree that your work is licensed under the
[Mozilla Public License 2.0](https://github.com/sustentabilitas/synapse-gateway/blob/main/LICENSE)
(MPL-2.0), the same licence as the rest of the project. If you have a question, open a
discussion on GitHub or use the contact in the [security policy](../operating/security.md).

MPL-2.0 welcomes commercial use: you can build Synapse into commercial and closed-source
products, and the licence covers only Synapse's own files, not the code you combine them with.
If you distribute a modified Synapse file, its source must stay available under MPL-2.0. We
ask that you send those changes back as a pull request, so every user benefits from them.
