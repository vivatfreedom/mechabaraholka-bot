FROM rust:1.88-slim-bookworm AS builder

WORKDIR /opt/app

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential ca-certificates pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/app
COPY --from=builder /opt/app/target/release/mechabaraholka-bot /usr/local/bin/mechabaraholka-bot

CMD ["mechabaraholka-bot"]
