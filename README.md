# Synapse

A Cargo workspace of Synapse services.

| Crate | Description | crates.io | Image |
|-------|-------------|-----------|-------|
| [`synapse-gateway`](crates/synapse-gateway) | OpenAI-compatible LLM router/gateway: streaming, tool calling, multi-provider fallback, native Vertex AI, per-tenant cost accounting, input guardrails. | `synapse-gateway` | `sustentabilitas/synapse-gateway` |
| [`synapse-proxy`](crates/synapse-proxy) | Config-driven reverse-proxy sidecar: path-prefix routing, static header injection, streaming passthrough. | `synapse-proxy` | `sustentabilitas/synapse-proxy` |

## Releasing

Each crate versions and releases independently via a prefixed git tag:

- `synapse-gateway-vX.Y.Z` → publishes the gateway crate + image.
- `synapse-proxy-vX.Y.Z` → publishes the proxy crate + image.

See each crate's README for configuration and usage.
