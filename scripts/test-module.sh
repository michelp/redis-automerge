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

# Test nested path operations
echo "9. Testing nested path operations..."
redis-cli -h "$HOST" del doc2
redis-cli -h "$HOST" am.new doc2

# Test nested text paths
redis-cli -h "$HOST" am.puttext doc2 user.profile.name "Bob"
val=$(redis-cli -h "$HOST" --raw am.gettext doc2 user.profile.name)
test "$val" = "Bob"
echo "   ✓ Nested text paths work"

# Test nested int paths
redis-cli -h "$HOST" am.putint doc2 user.profile.age 25
val=$(redis-cli -h "$HOST" am.getint doc2 user.profile.age)
test "$val" = "25"
echo "   ✓ Nested integer paths work"

# Test nested double paths
redis-cli -h "$HOST" am.putdouble doc2 metrics.cpu.usage 75.5
val=$(redis-cli -h "$HOST" am.getdouble doc2 metrics.cpu.usage)
test "$val" = "75.5"
echo "   ✓ Nested double paths work"

# Test nested bool paths
redis-cli -h "$HOST" am.putbool doc2 flags.features.enabled true
val=$(redis-cli -h "$HOST" am.getbool doc2 flags.features.enabled)
test "$val" = "1"
echo "   ✓ Nested boolean paths work"

# Test JSONPath-style with $ prefix
echo "10. Testing JSONPath-style paths with $ prefix..."
redis-cli -h "$HOST" del doc3
redis-cli -h "$HOST" am.new doc3
redis-cli -h "$HOST" am.puttext doc3 '$.user.name' "Charlie"
val=$(redis-cli -h "$HOST" --raw am.gettext doc3 '$.user.name')
test "$val" = "Charlie"
# Verify the same path works without $
val=$(redis-cli -h "$HOST" --raw am.gettext doc3 user.name)
test "$val" = "Charlie"
echo "   ✓ JSONPath-style $ prefix works"

# Test deeply nested paths
echo "11. Testing deeply nested paths..."
redis-cli -h "$HOST" del doc4
redis-cli -h "$HOST" am.new doc4
redis-cli -h "$HOST" am.puttext doc4 a.b.c.d.e.f.value "deeply nested"
val=$(redis-cli -h "$HOST" --raw am.gettext doc4 a.b.c.d.e.f.value)
test "$val" = "deeply nested"
echo "   ✓ Deeply nested paths work"

# Test persistence of nested paths
echo "12. Testing persistence of nested paths..."
redis-cli -h "$HOST" --raw am.save doc2 > /tmp/nested-saved.bin
truncate -s -1 /tmp/nested-saved.bin
redis-cli -h "$HOST" del doc2
redis-cli -h "$HOST" --raw -x am.load doc2 < /tmp/nested-saved.bin

val=$(redis-cli -h "$HOST" --raw am.gettext doc2 user.profile.name)
test "$val" = "Bob"
val=$(redis-cli -h "$HOST" am.getint doc2 user.profile.age)
test "$val" = "25"
val=$(redis-cli -h "$HOST" am.getdouble doc2 metrics.cpu.usage)
test "$val" = "75.5"
val=$(redis-cli -h "$HOST" am.getbool doc2 flags.features.enabled)
test "$val" = "1"
echo "   ✓ Nested paths persist and reload correctly"

# Test mixing flat and nested keys
echo "13. Testing mixed flat and nested keys..."
redis-cli -h "$HOST" del doc5
redis-cli -h "$HOST" am.new doc5
redis-cli -h "$HOST" am.puttext doc5 simple "flat value"
redis-cli -h "$HOST" am.puttext doc5 nested.key "nested value"
val1=$(redis-cli -h "$HOST" --raw am.gettext doc5 simple)
val2=$(redis-cli -h "$HOST" --raw am.gettext doc5 nested.key)
test "$val1" = "flat value"
test "$val2" = "nested value"
echo "   ✓ Mixed flat and nested keys work"

echo ""
echo "✅ All integration tests passed!"
