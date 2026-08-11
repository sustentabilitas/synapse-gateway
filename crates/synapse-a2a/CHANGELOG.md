# Changelog

## Unreleased

- Static seed from `[[a2a_agents]]` TOML (`seed_from_path`); cards fetched from `card_url` at boot with retry.
- **Breaking:** registration is insert-only (first-writer-wins). Duplicate `POST` / seed id no longer hot-swaps; returns success and keeps the first entry.

## 0.1.0

- Initial A2A agent registry: admin register/deregister, public catalog/card/resolve, in-memory TTL registry.
