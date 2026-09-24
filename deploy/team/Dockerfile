ARG RUST_VERSION=1.98.1
FROM rust:${RUST_VERSION}-bookworm AS builder

# Debian mirror acceleration for mainland China build environments.
RUN sed -i 's@deb.debian.org@mirrors.aliyun.com@g; s@security.debian.org@mirrors.aliyun.com@g' /etc/apt/sources.list.d/debian.sources 2>/dev/null || true \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Rust crates mirror. Keep lockfile resolution unchanged.
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_REGISTRIES_CRATES_IO_INDEX=https://rsproxy.cn/index/

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY apps ./apps
RUN cargo build --locked --release -p aiks-service

FROM debian:bookworm-slim AS runtime

RUN sed -i 's@deb.debian.org@mirrors.aliyun.com@g; s@security.debian.org@mirrors.aliyun.com@g' /etc/apt/sources.list.d/debian.sources 2>/dev/null || true \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/aiks-service /usr/local/bin/aiks-service
COPY deploy/team/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod 0555 /usr/local/bin/aiks-service /usr/local/bin/docker-entrypoint.sh

WORKDIR /var/lib/aiks-team
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
