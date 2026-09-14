# ThePenMarket.com rebuild: one static-ish Rust binary + templates + static + media.
# Build context: the server/ directory. Media (product photos) is mounted at /app/media; the database is external.
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
COPY tests ./tests
RUN cargo build --release --bin thepenmarket --bin import --bin admin

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/target/release/thepenmarket /src/target/release/import /src/target/release/admin /app/
COPY migrations ./migrations
COPY templates ./templates
COPY static ./static
COPY data ./data
RUN mkdir -p /app/media /app/uploads
ENV APP_DIR=/app PORT=8140 HOST=0.0.0.0
EXPOSE 8140
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD curl -sf http://127.0.0.1:8140/healthz | grep -q ok || exit 1
CMD ["/app/thepenmarket"]
