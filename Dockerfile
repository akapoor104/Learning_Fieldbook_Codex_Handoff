FROM rust:1.85.1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/fieldbook crates/fieldbook
COPY migrations migrations
RUN cargo build --release --locked -p fieldbook

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && install -d -o 10001 -g 10001 -m 0700 /var/data/fieldbook
COPY --from=build /src/target/release/fieldbook /usr/local/bin/fieldbook
USER 10001:10001
ENV PORT=8080
CMD ["fieldbook", "serve"]
