#!/usr/bin/env bash
# Verify every workspace crate that <crate> depends on is already published on
# crates.io at its current version.
#
# `cargo publish` resolves a path+version dependency against the registry, not
# the local path. Publishing a crate before its workspace dependencies therefore
# fails deep in packaging with a bare "failed to select a version for the
# requirement" error that names no remedy. This runs the same check up front and
# says exactly which crate to release first.
#
# Path dependencies carrying no version requirement are skipped: cargo strips
# those at publish time, so they never need to be on crates.io.
set -euo pipefail

CRATE="${1:?usage: check-path-deps-published.sh <crate>}"
UA="${CRATES_IO_USER_AGENT:-synapse-release-check (https://github.com/sustentabilitas/synapse-gateway)}"

META="$(cargo metadata --no-deps --format-version 1)"

version_of() {
  jq -r --arg c "$1" '.packages[] | select(.name==$c) | .version' <<<"$META"
}

path_deps_of() {
  jq -r --arg c "$1" '
    .packages[] | select(.name==$c) | .dependencies[]
    | select(.path != null and .req != "*") | .name' <<<"$META"
}

# Transitive closure via a worklist — a workspace dependency may itself depend
# on another one (synapse-mcp -> synapse-context).
seen=""
queue="$(path_deps_of "$CRATE")"
while [ -n "${queue// /}" ]; do
  next=""
  for dep in $queue; do
    case " $seen " in
    *" $dep "*) continue ;;
    esac
    seen="$seen $dep"
    next="$next $(path_deps_of "$dep")"
  done
  queue="$next"
done

if [ -z "${seen// /}" ]; then
  echo "${CRATE} has no workspace path dependencies to check"
  exit 0
fi

missing=0
for dep in $seen; do
  ver="$(version_of "$dep")"
  code="$(curl -s -o /dev/null -w '%{http_code}' -H "User-Agent: ${UA}" \
    "https://crates.io/api/v1/crates/${dep}/${ver}")"
  case "$code" in
  200)
    echo "ok: ${dep} ${ver} is on crates.io"
    ;;
  404)
    echo "::error::${dep} ${ver} is not on crates.io, but ${CRATE} depends on it. Release ${dep} first — push tag ${dep}-v${ver}, or run release-libs.yml with crate=${dep} — then retry this release."
    missing=1
    ;;
  *)
    echo "::error::crates.io lookup for ${dep} ${ver} returned HTTP ${code}"
    missing=1
    ;;
  esac
done

exit "$missing"
