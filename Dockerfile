# Stage 1: Compute recipe (dependency lock file)
FROM lukemathwalker/cargo-chef:latest-rust-1-alpine AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Build dependencies (cached layer - this takes longest!)
FROM chef AS builder 
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Stage 3: Build the actual application
COPY . .
RUN cargo build --release --bin discord-role-restore

# Stage 4: Tiny Alpine runtime
FROM alpine:latest AS runtime
WORKDIR /app

# Discord bots need these for SSL/TLS connections
RUN apk add --no-cache ca-certificates libgcc

# Copy the binary from builder
COPY --from=builder /app/target/release/discord-role-restore .

# Create a non-root user for security
RUN adduser -D -u 1000 bot && \
    chown -R bot:bot /app

USER bot

LABEL maintainer="Discord Role Restore Bot"
LABEL description="Discord bot that automatically restores user roles when members rejoin"

CMD ["./discord-role-restore"]
