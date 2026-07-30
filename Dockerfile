# Multi-stage Dockerfile for Kugel Collaboration Server
FROM rust:1-slim as builder

WORKDIR /usr/src/kugel

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy manifest and source
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build server release binary
RUN cargo build --release --bin kugel-server

# Production runtime stage
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/kugel/target/release/kugel-server /usr/local/bin/kugel-server

ENV PORT=8765
ENV HOST=0.0.0.0
ENV DATABASE_PATH=/app/data/kugel.db

EXPOSE 8765

VOLUME ["/app/data"]

CMD ["kugel-server"]
