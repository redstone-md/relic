# syntax=docker/dockerfile:1

# The panel is a static bundle the daemon carries inside its binary.
FROM node:22-bookworm-slim AS web
WORKDIR /src/web
RUN npm install -g pnpm@9
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY --from=web /src/web/dist web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p admin-core \
    && cp target/release/admin-core /admin-core

FROM debian:bookworm-slim
# Java is not installed here: the daemon downloads the Temurin runtime each
# Minecraft version needs into its data directory.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /data --shell /usr/sbin/nologin relic
COPY --from=build /admin-core /usr/local/bin/admin-core
USER relic
WORKDIR /data
ENV RELIC_DATA_DIR=/data \
    RELIC_BIND=0.0.0.0:8080
VOLUME /data
# 8080 is the panel and launcher API; game ports are whatever the instances
# are given, 25565 being the usual first one.
EXPOSE 8080 25565
ENTRYPOINT ["admin-core"]
