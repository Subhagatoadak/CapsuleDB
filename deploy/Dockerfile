# capsule-api container image (Phase 10). Multi-stage: build the whole
# workspace in a Rust image (rusqlite's `bundled` feature compiles SQLite
# from source, so the builder stage needs a C compiler), then ship only
# the resulting binary in a slim runtime image.

FROM rust:1-bookworm AS builder
WORKDIR /build

# Copy the whole workspace — capsule-api depends on capsule-core,
# capsule-registry, capsule-resolver, capsule-runtime, and
# capsule-evaluator via path dependencies, so a partial copy won't build.
COPY Cargo.toml Cargo.lock ./
COPY capsule-core ./capsule-core
COPY capsule-registry ./capsule-registry
COPY capsule-resolver ./capsule-resolver
COPY capsule-runtime ./capsule-runtime
COPY capsule-evaluator ./capsule-evaluator
COPY capsule-api ./capsule-api
COPY capsule-compiler ./capsule-compiler
COPY capsule-observatory ./capsule-observatory
COPY capsule-cli ./capsule-cli
COPY capsule-policy ./capsule-policy
COPY connector-sdk ./connector-sdk

RUN cargo build --release -p capsule-api

FROM debian:bookworm-slim AS runtime
RUN useradd --system --create-home --home-dir /data capsule
COPY --from=builder /build/target/release/capsule-api /usr/local/bin/capsule-api

USER capsule
WORKDIR /data
VOLUME ["/data"]
EXPOSE 8089

ENV RUST_LOG=capsule_api=info

ENTRYPOINT ["capsule-api"]
CMD ["--db", "/data/capsuledb.sqlite3", "--addr", "0.0.0.0:8089"]
