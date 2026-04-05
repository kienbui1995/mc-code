# === Dev target: cargo watch, debug build ===
FROM rust:1.75-slim AS dev
RUN apt-get update && apt-get install -y pkg-config libssl-dev git && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
CMD ["cargo", "run"]

# === Build target: release binary ===
FROM rust:1.75-slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
RUN cargo build --release && strip target/release/magic-code

# === Prod target: minimal runtime ===
FROM debian:bookworm-slim AS prod
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/magic-code /usr/local/bin/magic-code
RUN useradd -m mcuser
USER mcuser
WORKDIR /home/mcuser/workspace
ENTRYPOINT ["magic-code"]
