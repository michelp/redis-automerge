#!/usr/bin/env bash
set -euo pipefail
HOST="${REDIS_HOST:-127.0.0.1}"

echo "Testing Redis Automerge Module..."

# Ensure server is up
echo "1. Checking server connection..."
redis-cli -h "$HOST" ping
redis-cli -h "$HOST" del doc

# Create a new document and test text values
echo "2. Testing text operations..."
redis-cli -h "$HOST" am.new doc
redis-cli -h "$HOST" am.puttext doc greeting "hello world"
val=$(redis-cli -h "$HOST" --raw am.gettext doc greeting)
test "$val" = "hello world"
echo "   ✓ Text get/set works"

# Test integer operations
echo "3. Testing integer operations..."
redis-cli -h "$HOST" am.putint doc age 42
val=$(redis-cli -h "$HOST" am.getint doc age)
test "$val" = "42"
echo "   ✓ Integer get/set works"

# Test negative integers
redis-cli -h "$HOST" am.putint doc temperature -10
val=$(redis-cli -h "$HOST" am.getint doc temperature)
test "$val" = "-10"
echo "   ✓ Negative integers work"

# Test double operations
echo "4. Testing double operations..."
redis-cli -h "$HOST" am.putdouble doc pi 3.14159
val=$(redis-cli -h "$HOST" am.getdouble doc pi)
test "$val" = "3.14159"
echo "   ✓ Double get/set works"

# Test boolean operations
echo "5. Testing boolean operations..."
redis-cli -h "$HOST" am.putbool doc active true
val=$(redis-cli -h "$HOST" am.getbool doc active)
test "$val" = "1"
echo "   ✓ Boolean true works"

redis-cli -h "$HOST" am.putbool doc disabled false
val=$(redis-cli -h "$HOST" am.getbool doc disabled)
test "$val" = "0"
echo "   ✓ Boolean false works"

# Test mixed types in same document
echo "6. Testing mixed types..."
redis-cli -h "$HOST" am.puttext doc name "Alice"
redis-cli -h "$HOST" am.putint doc count 100
redis-cli -h "$HOST" am.putdouble doc score 95.5
redis-cli -h "$HOST" am.putbool doc verified 1

name=$(redis-cli -h "$HOST" --raw am.gettext doc name)
count=$(redis-cli -h "$HOST" am.getint doc count)
score=$(redis-cli -h "$HOST" am.getdouble doc score)
verified=$(redis-cli -h "$HOST" am.getbool doc verified)

test "$name" = "Alice"
test "$count" = "100"
test "$score" = "95.5"
test "$verified" = "1"
echo "   ✓ Mixed types in single document work"

# Persist and reload the document with all types
echo "7. Testing persistence with all types..."
redis-cli -h "$HOST" --raw am.save doc > /tmp/saved.bin
truncate -s -1 /tmp/saved.bin
redis-cli -h "$HOST" del doc
redis-cli -h "$HOST" --raw -x am.load doc < /tmp/saved.bin

# Verify all values persisted correctly
val=$(redis-cli -h "$HOST" --raw am.gettext doc greeting)
test "$val" = "hello world"

val=$(redis-cli -h "$HOST" am.getint doc age)
test "$val" = "42"

val=$(redis-cli -h "$HOST" am.getdouble doc pi)
test "$val" = "3.14159"

val=$(redis-cli -h "$HOST" am.getbool doc active)
test "$val" = "1"

name=$(redis-cli -h "$HOST" --raw am.gettext doc name)
count=$(redis-cli -h "$HOST" am.getint doc count)
score=$(redis-cli -h "$HOST" am.getdouble doc score)
verified=$(redis-cli -h "$HOST" am.getbool doc verified)

test "$name" = "Alice"
test "$count" = "100"
test "$score" = "95.5"
test "$verified" = "1"
echo "   ✓ All types persist and reload correctly"

# Test non-existent fields return null
echo "8. Testing null returns for non-existent fields..."
val=$(redis-cli -h "$HOST" am.gettext doc nonexistent)
test "$val" = ""
val=$(redis-cli -h "$HOST" am.getint doc nonexistent)
test "$val" = ""
echo "   ✓ Non-existent fields return null"

echo ""
echo "✅ All integration tests passed!"
