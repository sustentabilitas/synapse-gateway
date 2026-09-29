# Contributing to Synapse

Thank you for your interest in contributing to Synapse! This is an open-source,
MPL-2.0-licensed project and we welcome issues, bug reports, feature requests, and pull
requests from the community.

Before you begin, please read our [Code of Conduct](CODE_OF_CONDUCT.md) and
[Security Policy](SECURITY.md). By participating you agree to abide by the Code of Conduct.

The full contributor guide lives on the documentation site:
[Contributing](https://synapse-gateway.readthedocs.io/en/latest/docs/contributing/) and, for
maintainers, [Releasing](https://synapse-gateway.readthedocs.io/en/latest/docs/contributing/releasing/).
This file is the short version.

---

## Table of Contents

1. [Getting started](#getting-started)
2. [Before you submit](#before-you-submit)
3. [Development workflow](#development-workflow)
4. [Documentation](#documentation)
5. [Commit messages](#commit-messages)
6. [Developer Certificate of Origin (DCO)](#developer-certificate-of-origin-dco)
7. [Pull requests](#pull-requests)
8. [Releasing](#releasing)
9. [License](#license)

---

## Getting started

### Prerequisites

- The current stable [Rust toolchain](https://rustup.rs/), with `rustfmt` and `clippy` (both
  ship with `rustup`). The repository doesn't pin a toolchain; CI uses the latest stable.
- Optional: Docker, to build the gateway and proxy images.
- Optional: Node.js 22, to work on the documentation site.

### Clone and build

```bash
git clone https://github.com/sustentabilitas/synapse-gateway.git
cd synapse-gateway

# Build every crate in the workspace (gateway default features: server + ledger-sqlite)
cargo build

# Run the test suite
cargo test
```

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

---

## Before you submit

Run all of the following locally before opening a pull request. CI enforces each gate and
a red build blocks merging.

```bash
# 1. Format check (must produce no diff)
cargo fmt --all --check

# 2. Lint (zero warnings, warnings treated as errors)
cargo clippy --all-targets -- -D warnings

# 3. Test suite with default features
cargo test

# 4. Build all feature variants your change touches
cargo build -p synapse-gateway --features ledger-postgres
cargo build -p synapse-gateway --features ledger-pubsub
cargo build -p synapse-gateway --features ledger-sns
cargo build -p synapse-gateway --features "ledger-pubsub ledger-sns"
cargo build -p synapse-gateway --no-default-features --lib
```

Also:

- **Update the changelog**: each crate has its own `crates/<crate>/CHANGELOG.md`. For the
  gateway, add a line under `## [Unreleased]` in the appropriate group (`Added`, `Changed`,
  `Fixed`, `Removed`, `Security`). See
  [Before you submit](https://synapse-gateway.readthedocs.io/en/latest/docs/contributing/#before-you-submit)
  for the other crates.
- **Update documentation**: if your change affects public API, configuration, HTTP endpoints,
  metrics, or behaviour, update the pages on the documentation site (see
  [Documentation](#documentation)) and the rustdoc comments.

---

## Development workflow

Non-trivial contributions — new features, significant refactors, new ledger backends, changes
to the public library API — should follow the **spec → plan → implement** flow used in this
project.

1. **Write a spec**. Describe *what* and *why*: the problem, proposed behaviour, edge cases,
   and acceptance criteria. Keep it concise.

2. **Write a plan**. Break the work into small, reviewable steps. The plan references the
   spec.

3. **Implement with TDD**: write the failing test first (in `tests/` or as a `#[cfg(test)]`
   module in the relevant source file), then write the minimal implementation that makes it
   pass, then refactor. Commit the test separately from the implementation where it aids
   reviewability.

4. **Open a PR** that links to or summarises the spec and plan so reviewers have full context.

Specs and plans are local working documents: keep them under `docs/superpowers/`, which is
gitignored, and don't commit them. Small bug fixes and documentation improvements do not need
a full spec; use your judgement.

---

## Documentation

The documentation site, https://synapse-gateway.readthedocs.io/en/latest/, is a Docusaurus
project in `docs/`, in English and Spanish. It is the canonical documentation; the READMEs
only link into it. To run it locally with live reload:

```bash
cd docs && npm ci && npm start
```

- **Spanish parity**: a change to an English page under `docs/docs/` updates its Spanish twin
  under `docs/i18n/es/docusaurus-plugin-content-docs/current/`, at the same relative path.
  `npm run check:i18n` fails when a page exists in one language and not the other.
- **Titled config examples are tested**: a code block fenced with a title ending in
  `routes.toml`, `pricing.toml`, `guardrails.toml` or `ai_task_types.toml` must be a complete,
  valid file. `cargo test -p synapse-gateway --test docs_examples` parses every one of them
  with the gateway's own parsers. Fence partial fragments without a title.
- **Before you open a PR** that touches `docs/`, run `npm test`, `npm run check:i18n`,
  `npm run typecheck` and `npm run build` (which fails on broken links and anchors).
- `docs/superpowers/` is local-only and ignored; never commit anything under it.

See [Working on the docs](https://synapse-gateway.readthedocs.io/en/latest/docs/contributing/#working-on-the-docs)
for page conventions.

---

## Commit messages

- Use an **imperative, present-tense subject line** (e.g. `add Pub/Sub ledger sink`, not
  `added` or `adding`).
- Keep the subject under **72 characters**.
- Use conventional-commit prefixes, scoped to the crate you change where that helps (e.g.
  `fix(synapse-proxy): ...`):

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

- Optionally include a body (blank line after subject) explaining *why* the change was made.
- Reference related issues or PRs at the bottom: `Closes #42`.

Example:

```
feat(synapse-gateway): add AWS SNS ledger sink

Adds a fan-out sink that publishes cost-ledger events to an SNS topic.
Gated behind the `ledger-sns` feature flag.

Closes #17
Signed-off-by: Your Name <your@email.com>
```

---

## Developer Certificate of Origin (DCO)

**Every commit must carry a `Signed-off-by` trailer.**

By signing off you certify that you have the right to submit the contribution under the
project's MPL-2.0 license, as defined by the Developer Certificate of Origin at
<https://developercertificate.org/>.

Add the sign-off automatically with the `-s` flag:

```bash
git commit -s -m "feat: your change description"
```

This appends a line like:

```
Signed-off-by: Your Name <your@email.com>
```

The name and email must match a real identity (your real name and a working email address).

> **Pull requests that contain unsigned commits will not be merged.** If you forget to sign
> off on earlier commits, you can amend them:
>
> ```bash
> # Amend the most recent commit
> git commit --amend -s --no-edit
>
> # Sign off all commits in the branch at once (rebase approach)
> git rebase --signoff HEAD~<N>
> ```

---

## Pull requests

1. **Fork** the repository and create a feature branch from `main`.
2. Follow the [development workflow](#development-workflow) and
   [commit guidelines](#commit-messages).
3. Ensure **all CI checks pass** before requesting review.
4. Open a pull request with:
   - A clear title (conventional-commit style).
   - A description explaining *what* changed and *why*.
   - Links to or a summary of the spec and plan for non-trivial changes.
   - `Closes #<issue>` if applicable.
5. Address reviewer feedback promptly. One approving review from a maintainer is required
   to merge.
6. Maintainers may squash or rebase on merge to keep the history clean.

---

## Releasing

Maintainers only. Each crate is versioned and released on its own, with a tag named after the
crate: `<crate>-vX.Y.Z` (for example `synapse-gateway-v2.0.0`). The usual way to release is
the crate's **Bump & release** workflow in the Actions tab (**Bump & release synapse-gateway**,
**Bump & release synapse-proxy**, and so on), which bumps the version and the crate's
changelog, commits, tags, and starts the release.

For a tag, [`release.yml`](.github/workflows/release.yml) publishes `synapse-gateway` and
`synapse-proxy` to crates.io and pushes their Docker images;
[`release-libs.yml`](.github/workflows/release-libs.yml) publishes `synapse-a2a`,
`synapse-mcp` and `synapse-context` to crates.io.

See [Releasing](https://synapse-gateway.readthedocs.io/en/latest/docs/contributing/releasing/)
for the full steps, publish order and required secrets.

---

## License

By submitting a contribution you agree that your work is licensed under the
[Mozilla Public License 2.0](LICENSE) (MPL-2.0), the same license as the rest
of the project.

If you have any questions, feel free to open a discussion or reach out via the contact in
[SECURITY.md](SECURITY.md).
