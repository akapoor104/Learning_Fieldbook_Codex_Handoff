FROM node:24.12.0-bookworm-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/index.html web/vite.config.js ./
COPY web/src src
RUN npm run build

FROM rust:1.88.0-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/fieldbook crates/fieldbook
COPY migrations migrations
RUN cargo build --release --locked -p fieldbook

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o 10001 -g 10001 -m 0700 /var/data/fieldbook
COPY --from=build /src/target/release/fieldbook /usr/local/bin/fieldbook
COPY --from=web /src/web/dist /opt/fieldbook/web
ENV FIELD_WEB_ROOT=/opt/fieldbook/web
USER 10001:10001
ENV PORT=8080
CMD ["fieldbook", "serve"]
