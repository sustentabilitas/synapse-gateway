# ---- build stage ----
FROM rust:1-bookworm AS build
WORKDIR /app
COPY . .
RUN cargo build --release --locked -p synapse-proxy --bin synapse-proxy

# ---- runtime stage ----
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --gid 1001 synapse \
 && useradd --uid 1001 --gid synapse --shell /usr/sbin/nologin --no-create-home synapse
WORKDIR /app
COPY --from=build /app/target/release/synapse-proxy /usr/local/bin/synapse-proxy
COPY crates/synapse-proxy/synapse-proxy.toml /app/synapse-proxy.toml
RUN chown -R synapse:synapse /app
USER synapse
EXPOSE 8787
ENTRYPOINT ["synapse-proxy"]
