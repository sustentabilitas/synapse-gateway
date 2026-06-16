#!/usr/bin/env bash
set -euo pipefail

url="${GATEWAY_URL:-http://gateway:8080}/health"
deadline="${GATEWAY_WAIT_SECS:-120}"

echo "waiting for gateway at ${url} (up to ${deadline}s)..."
for ((i = 1; i <= deadline; i++)); do
  if curl -sf "$url" >/dev/null; then
    echo "gateway is up"
    exit 0
  fi
  sleep 1
done

echo "gateway did not become healthy within ${deadline}s" >&2
exit 1
