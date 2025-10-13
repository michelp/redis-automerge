# Build Redis module
FROM rust:1 AS builder
RUN apt-get update && apt-get install -y clang && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY redis-automerge/ ./redis-automerge/
RUN cargo build --release --manifest-path redis-automerge/Cargo.toml

# Runtime image with Redis and the compiled module
FROM redis:7
COPY --from=builder /build/redis-automerge/target/release/libredis_automerge.so /usr/lib/redis/modules/redis-automerge.so
CMD ["redis-server", \
     "--loadmodule", "/usr/lib/redis/modules/redis-automerge.so", \
     "--loglevel", "notice", \
     "--logfile", "", \
     "--slowlog-log-slower-than", "0", \
     "--slowlog-max-len", "128", \
     "--notify-keyspace-events", "KEA"]
