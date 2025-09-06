#!/usr/bin/env bash
set -euo pipefail
HOST="${REDIS_HOST:-127.0.0.1}"

# Ensure server is up
redis-cli -h "$HOST" ping > /dev/null
redis-cli -h "$HOST" del doc > /dev/null

# Create a new document and round-trip a text value
redis-cli -h "$HOST" am.new doc > /dev/null
redis-cli -h "$HOST" am.puttext doc greeting "hello world" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext doc greeting)
test "$val" = "hello world"

# Persist and reload the document
redis-cli -h "$HOST" --raw am.save doc > /tmp/saved.bin
truncate -s -1 /tmp/saved.bin
redis-cli -h "$HOST" del doc > /dev/null
redis-cli -h "$HOST" --raw -x am.load doc < /tmp/saved.bin
val=$(redis-cli -h "$HOST" --raw am.gettext doc greeting)
test "$val" = "hello world"
