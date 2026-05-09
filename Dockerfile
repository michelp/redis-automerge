# Build Redis module against the published `automerge` crate from crates.io.
FROM rust:1 AS builder

# `redis-module`'s build script uses bindgen, which requires libclang.
RUN apt-get update && apt-get install -y clang && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy our redis-automerge module code (Cargo.toml + Cargo.lock pin the
# exact dependency tree so this build is reproducible).
COPY redis-automerge/ ./redis-automerge/

# Build the Redis module
RUN cargo build --release --manifest-path redis-automerge/Cargo.toml

# Runtime image with Redis and the compiled module
FROM redis:7
COPY --from=builder /build/redis-automerge/target/release/libredis_automerge.so /usr/lib/redis/modules/redis-automerge.so
# NOTE: keep this flag list in sync with the `command:` override on the `redis`
# service in docker-compose.yml, which re-applies the same flags plus
# `--enable-debug-command yes` for the local/test stack.
CMD ["redis-server", \
     "--loadmodule", "/usr/lib/redis/modules/redis-automerge.so", \
     "--loglevel", "notice", \
     "--logfile", "", \
     "--slowlog-log-slower-than", "0", \
     "--slowlog-max-len", "128", \
     "--notify-keyspace-events", "KEA", \
     "--dir", "/data", \
     "--save", "", \
     "--appendonly", "yes", \
     "--appendfilename", "appendonly.aof", \
     "--appendfsync", "everysec", \
     "--aof-use-rdb-preamble", "no"]
