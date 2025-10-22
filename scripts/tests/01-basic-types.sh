#!/usr/bin/env bash
# Test basic type operations: text, int, double, bool, counter

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Basic Type Operations"

# Create a new document and test text values
echo "Test 1: Text operations..."
redis-cli -h "$HOST" am.new doc > /dev/null
redis-cli -h "$HOST" am.puttext doc greeting "hello world" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext doc greeting)
assert_equals "$val" "hello world"
echo "   ✓ Text get/set works"

# Test integer operations
echo "Test 2: Integer operations..."
redis-cli -h "$HOST" am.putint doc age 42 > /dev/null
val=$(redis-cli -h "$HOST" am.getint doc age)
assert_equals "$val" "42"
echo "   ✓ Integer get/set works"

# Test negative integers
redis-cli -h "$HOST" am.putint doc temperature -10 > /dev/null
val=$(redis-cli -h "$HOST" am.getint doc temperature)
assert_equals "$val" "-10"
echo "   ✓ Negative integers work"

# Test double operations
echo "Test 3: Double operations..."
redis-cli -h "$HOST" am.putdouble doc pi 3.14159 > /dev/null
val=$(redis-cli -h "$HOST" am.getdouble doc pi)
assert_equals "$val" "3.14159"
echo "   ✓ Double get/set works"

# Test boolean operations
echo "Test 4: Boolean operations..."
redis-cli -h "$HOST" am.putbool doc active true > /dev/null
val=$(redis-cli -h "$HOST" am.getbool doc active)
assert_equals "$val" "1"
echo "   ✓ Boolean true works"

redis-cli -h "$HOST" am.putbool doc disabled false > /dev/null
val=$(redis-cli -h "$HOST" am.getbool doc disabled)
assert_equals "$val" "0"
echo "   ✓ Boolean false works"

# Test counter operations
echo "Test 5: Counter operations..."
redis-cli -h "$HOST" am.putcounter doc views 0 > /dev/null
val=$(redis-cli -h "$HOST" am.getcounter doc views)
assert_equals "$val" "0"
echo "   ✓ Counter get/set works"

redis-cli -h "$HOST" am.inccounter doc views 5 > /dev/null
val=$(redis-cli -h "$HOST" am.getcounter doc views)
assert_equals "$val" "5"
echo "   ✓ Counter increment works"

redis-cli -h "$HOST" am.inccounter doc views 3 > /dev/null
val=$(redis-cli -h "$HOST" am.getcounter doc views)
assert_equals "$val" "8"
echo "   ✓ Counter multiple increments work"

redis-cli -h "$HOST" am.inccounter doc views -2 > /dev/null
val=$(redis-cli -h "$HOST" am.getcounter doc views)
assert_equals "$val" "6"
echo "   ✓ Counter decrement (negative increment) works"

# Test mixed types in same document
echo "Test 6: Mixed types..."
redis-cli -h "$HOST" am.puttext doc name "Alice" > /dev/null
redis-cli -h "$HOST" am.putint doc count 100 > /dev/null
redis-cli -h "$HOST" am.putdouble doc score 95.5 > /dev/null
redis-cli -h "$HOST" am.putbool doc verified 1 > /dev/null
redis-cli -h "$HOST" am.putcounter doc likes 42 > /dev/null

name=$(redis-cli -h "$HOST" --raw am.gettext doc name)
count=$(redis-cli -h "$HOST" am.getint doc count)
score=$(redis-cli -h "$HOST" am.getdouble doc score)
verified=$(redis-cli -h "$HOST" am.getbool doc verified)
likes=$(redis-cli -h "$HOST" am.getcounter doc likes)

assert_equals "$name" "Alice"
assert_equals "$count" "100"
assert_equals "$score" "95.5"
assert_equals "$verified" "1"
assert_equals "$likes" "42"
echo "   ✓ Mixed types in single document work"

# Persist and reload the document with all types
echo "Test 7: Persistence with all types..."
redis-cli -h "$HOST" --raw am.save doc > /tmp/saved.bin
truncate -s -1 /tmp/saved.bin
redis-cli -h "$HOST" del doc > /dev/null
redis-cli -h "$HOST" --raw -x am.load doc < /tmp/saved.bin > /dev/null

# Verify all values persisted correctly
val=$(redis-cli -h "$HOST" --raw am.gettext doc greeting)
assert_equals "$val" "hello world"

val=$(redis-cli -h "$HOST" am.getint doc age)
assert_equals "$val" "42"

val=$(redis-cli -h "$HOST" am.getdouble doc pi)
assert_equals "$val" "3.14159"

val=$(redis-cli -h "$HOST" am.getbool doc active)
assert_equals "$val" "1"

name=$(redis-cli -h "$HOST" --raw am.gettext doc name)
count=$(redis-cli -h "$HOST" am.getint doc count)
score=$(redis-cli -h "$HOST" am.getdouble doc score)
verified=$(redis-cli -h "$HOST" am.getbool doc verified)
likes=$(redis-cli -h "$HOST" am.getcounter doc likes)

assert_equals "$name" "Alice"
assert_equals "$count" "100"
assert_equals "$score" "95.5"
assert_equals "$verified" "1"
assert_equals "$likes" "42"
echo "   ✓ All types persist and reload correctly"

# Test non-existent fields return null
echo "Test 8: Null returns for non-existent fields..."
val=$(redis-cli -h "$HOST" am.gettext doc nonexistent)
assert_equals "$val" ""

val=$(redis-cli -h "$HOST" am.getint doc nonexistent)
assert_equals "$val" ""
echo "   ✓ Non-existent fields return null"

rm -f /tmp/saved.bin

echo ""
echo "✅ All basic type tests passed!"
