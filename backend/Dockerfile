# syntax=docker/dockerfile:1

FROM rust:1-alpine AS build

RUN apk add --no-cache musl-dev

WORKDIR /build

# Cache dependencies: build with a stub main first.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --locked --release \
    && rm -rf src target/release/deps/ar_vanila_api*

COPY src ./src
RUN touch src/main.rs && cargo build --locked --release

FROM alpine:3.20

RUN addgroup -g 10001 appgroup \
    && adduser -D -u 10001 -G appgroup -H -s /sbin/nologin appuser

LABEL org.opencontainers.image.title="ar-vanila-api" \
      org.opencontainers.image.description="AR Vanila Matcher API (Rust/axum)" \
      org.opencontainers.image.source="https://github.com/vanila-id/findar"

COPY --from=build /build/target/release/ar-vanila-api /usr/local/bin/ar-vanila-api

WORKDIR /app
RUN mkdir -p /app/uploads /app/outputs \
    && chown -R appuser:appgroup /app

USER appuser

EXPOSE 9001

HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
  CMD wget -q --spider http://127.0.0.1:9001/api/health || exit 1

CMD ["ar-vanila-api"]
