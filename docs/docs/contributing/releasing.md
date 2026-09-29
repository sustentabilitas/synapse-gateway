---
sidebar_position: 2
title: Releasing
description: How maintainers version and release each Synapse crate to crates.io and Docker Hub with the bump and release workflows.
---

Each crate in the workspace is versioned and released on its own, with a tag named after the
crate: `synapse-gateway-vX.Y.Z`, `synapse-proxy-vX.Y.Z`, `synapse-a2a-vX.Y.Z`,
`synapse-mcp-vX.Y.Z` or `synapse-context-vX.Y.Z`. Releasing is for maintainers only.

## What a release publishes

| Crate | crates.io | Docker Hub | Release workflow |
|---|---|---|---|
| `synapse-gateway` | Yes | `sustentabilitas/synapse-gateway` | `release.yml` |
| `synapse-proxy` | Yes | `sustentabilitas/synapse-proxy` | `release.yml` |
| `synapse-a2a` | Yes | No | `release-libs.yml` |
| `synapse-mcp` | Yes | No | `release-libs.yml` |
| `synapse-context` | Yes | No | `release-libs.yml` |

For a tag `<crate>-vX.Y.Z`, the release workflow:

1. checks that version `X.Y.Z` of the crate isn't already on crates.io, and that the tag matches
   the `version` in `crates/<crate>/Cargo.toml`;
2. runs `cargo publish -p <crate> --locked`;
3. for the gateway and the proxy only, builds the crate's Dockerfile for `linux/amd64` and pushes
   `sustentabilitas/<crate>:X.Y.Z` and `:latest` to Docker Hub.

Separately, every push to `main` builds and pushes rolling `sustentabilitas/synapse-gateway:edge`
and `sustentabilitas/synapse-proxy:edge` images, without publishing a crate.

## Release with the bump workflow

The usual way to release is the **Bump & release** workflow for the crate, in the repository's
Actions tab: **Bump & release synapse-gateway**, **Bump & release synapse-proxy**, and so on.
Choose `patch`, `minor` or `major`; the workflow always works on `main`. It:

1. works out the next version from the crate's `Cargo.toml` or its latest tag, whichever is
   higher;
2. updates the `version` in `crates/<crate>/Cargo.toml`, refreshes `Cargo.lock` and adds a
   heading for the new version to `crates/<crate>/CHANGELOG.md`;
3. commits `chore(<crate>): release vX.Y.Z` on `main`, tags it `<crate>-vX.Y.Z` and pushes both;
4. starts the release workflow for that tag.

The bump workflow starts the release workflow itself because a tag pushed with the workflow's
own token doesn't trigger other workflows.

## Release from your machine

Pushing a tag from your machine triggers the release workflow directly. Bump the version and
changelog yourself first, then commit on `main`, tag and push:

```bash
git tag synapse-gateway-vX.Y.Z
git push origin synapse-gateway-vX.Y.Z
```

The release fails at the first check if the tag doesn't match the crate's `Cargo.toml`.

You can also run **Release** or **Release library crate** by hand from the Actions tab. From a
branch, it publishes the version in the crate's `Cargo.toml` and, for the binaries, pushes the
image without moving `latest`.

## Publish order

crates.io must already have the version of each workspace dependency a crate is published
against, so release dependencies first:

- `synapse-context` before `synapse-proxy` and `synapse-mcp`, which depend on it.
- `synapse-a2a` before `synapse-gateway`, which depends on it through the `server` feature.

## Repository secrets

The release workflows need these secrets, under
**Settings → Secrets and variables → Actions**:

| Secret | Used for |
|---|---|
| `CARGO_REGISTRY_TOKEN` | Publishing to crates.io |
| `DOCKERHUB_USERNAME` | Pushing images to Docker Hub |
| `DOCKERHUB_TOKEN` | Pushing images to Docker Hub |

See the [changelogs](./changelog.md) for what each release contains.
