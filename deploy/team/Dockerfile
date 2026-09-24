ARG RUST_VERSION=1.98.1
FROM rust:${RUST_VERSION}-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps
RUN cargo build --locked --release -p aiks-service

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/aiks-service /usr/local/bin/aiks-service
COPY deploy/team/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 0555 /usr/local/bin/aiks-service /usr/local/bin/docker-entrypoint.sh

WORKDIR /var/lib/aiks-team
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
