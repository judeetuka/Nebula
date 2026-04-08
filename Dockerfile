# syntax=docker/dockerfile:1

FROM rust:1.93-slim AS builder
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/ crates/
RUN cargo build --release -p nebula-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
RUN groupadd --gid 1001 nebula && useradd --uid 1001 --gid nebula --shell /bin/false --create-home nebula
WORKDIR /app
COPY --from=builder /build/target/release/nebula-server /app/nebula-server
RUN mkdir -p /app/config && chown -R nebula:nebula /app
USER nebula
EXPOSE 8080 1884 2333
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD curl -f http://localhost:8080/api/health || exit 1
ENTRYPOINT ["/app/nebula-server"]
CMD ["--config", "/app/config/server.production.toml"]
