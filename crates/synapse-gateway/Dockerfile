# ---- build stage ----
FROM rust:1-bookworm AS build
WORKDIR /app
# Build deps for the cloud-ledger feature trees (aws-lc-sys needs cmake; tonic/prost
# generated code needs no protoc, but install it defensively).
RUN apt-get update \
 && apt-get install -y --no-install-recommends cmake protobuf-compiler \
 && rm -rf /var/lib/apt/lists/*
COPY . .
# All ledger backends so one image serves any SYNAPSE_LEDGER_BACKENDS value.
# (--features is additive over the defaults: server + ledger-sqlite.)
RUN cargo build --release --locked -p synapse-gateway --bin synapse-gateway \
      --features "ledger-postgres ledger-pubsub ledger-sns"

# ---- runtime stage ----
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --gid 1001 synapse \
 && useradd --uid 1001 --gid synapse --shell /usr/sbin/nologin --no-create-home synapse
WORKDIR /app
COPY --from=build /app/target/release/synapse-gateway /usr/local/bin/synapse-gateway
COPY crates/synapse-gateway/config/ /app/config/
COPY crates/synapse-gateway/migrations/ /app/migrations/
# /app must be writable by the non-root user for the default SQLite path
# (sqlite://synapse.db?mode=rwc) when no external ledger is configured.
RUN chown -R synapse:synapse /app
USER synapse
EXPOSE 8080 9090
ENTRYPOINT ["synapse-gateway"]
