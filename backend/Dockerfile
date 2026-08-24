FROM rust:1-alpine AS build

RUN apk add --no-cache musl-dev

WORKDIR /build

# Cache dependencies: build with a stub main first.
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src target/release/deps/ar_vanila_api*

COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM alpine:3.20

RUN addgroup -g 10001 appgroup \
    && adduser -D -u 10001 -G appgroup appuser

COPY --from=build /build/target/release/ar-vanila-api /usr/local/bin/ar-vanila-api

WORKDIR /app
RUN mkdir -p /app/uploads /app/outputs \
    && chown -R appuser:appgroup /app

USER appuser

EXPOSE 9001

CMD ["ar-vanila-api"]
