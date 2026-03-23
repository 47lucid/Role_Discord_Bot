# Stage 1: Compute recipe (dependency lock file)
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Build dependencies (cached layer - this takes longest!)
FROM chef AS builder 
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Stage 3: Build the actual application
COPY . .
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo build --release --bin discord-role-restore

# Stage 4: Debian runtime
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Discord bots need these for SSL/TLS connections
RUN apt-get update && apt-get install -y ca-certificates openssl libssl3 && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/discord-role-restore .

# Create a non-root user for security
RUN useradd -m -u 1000 bot && \
    mkdir -p /app && \
    chown -R bot:bot /app

USER bot

# Environment variables for logging
ENV RUST_LOG=debug
ENV RUST_BACKTRACE=1
ENV RUST_ENVLOGGER_PADDING=0

LABEL maintainer="Discord Role Restore Bot"
LABEL description="Discord bot that automatically restores user roles when members rejoin"

# Run with unbuffered output
CMD ["./discord-role-restore"]
