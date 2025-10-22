#!/usr/bin/env bash
# Test nested path operations and JSONPath syntax

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Nested Path Operations"

# Test nested path operations
echo "Test 1: Nested path operations..."
redis-cli -h "$HOST" del doc2 > /dev/null
redis-cli -h "$HOST" am.new doc2 > /dev/null

# Test nested text paths
redis-cli -h "$HOST" am.puttext doc2 user.profile.name "Bob" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext doc2 user.profile.name)
assert_equals "$val" "Bob"
echo "   ✓ Nested text paths work"

# Test nested int paths
redis-cli -h "$HOST" am.putint doc2 user.profile.age 25 > /dev/null
val=$(redis-cli -h "$HOST" am.getint doc2 user.profile.age)
assert_equals "$val" "25"
echo "   ✓ Nested integer paths work"

# Test nested double paths
redis-cli -h "$HOST" am.putdouble doc2 metrics.cpu.usage 75.5 > /dev/null
val=$(redis-cli -h "$HOST" am.getdouble doc2 metrics.cpu.usage)
assert_equals "$val" "75.5"
echo "   ✓ Nested double paths work"

# Test nested bool paths
redis-cli -h "$HOST" am.putbool doc2 flags.features.enabled true > /dev/null
val=$(redis-cli -h "$HOST" am.getbool doc2 flags.features.enabled)
assert_equals "$val" "1"
echo "   ✓ Nested boolean paths work"

# Test nested counter paths
redis-cli -h "$HOST" am.putcounter doc2 stats.pageviews 100 > /dev/null
val=$(redis-cli -h "$HOST" am.getcounter doc2 stats.pageviews)
assert_equals "$val" "100"
redis-cli -h "$HOST" am.inccounter doc2 stats.pageviews 50 > /dev/null
val=$(redis-cli -h "$HOST" am.getcounter doc2 stats.pageviews)
assert_equals "$val" "150"
echo "   ✓ Nested counter paths work"

# Test JSONPath-style with $ prefix
echo "Test 2: JSONPath-style paths with $ prefix..."
redis-cli -h "$HOST" del doc3 > /dev/null
redis-cli -h "$HOST" am.new doc3 > /dev/null
redis-cli -h "$HOST" am.puttext doc3 '$.user.name' "Charlie" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext doc3 '$.user.name')
assert_equals "$val" "Charlie"
# Verify the same path works without $
val=$(redis-cli -h "$HOST" --raw am.gettext doc3 user.name)
assert_equals "$val" "Charlie"
echo "   ✓ JSONPath-style $ prefix works"

# Test deeply nested paths
echo "Test 3: Deeply nested paths..."
redis-cli -h "$HOST" del doc4 > /dev/null
redis-cli -h "$HOST" am.new doc4 > /dev/null
redis-cli -h "$HOST" am.puttext doc4 a.b.c.d.e.f.value "deeply nested" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext doc4 a.b.c.d.e.f.value)
assert_equals "$val" "deeply nested"
echo "   ✓ Deeply nested paths work"

# Test persistence of nested paths
echo "Test 4: Persistence of nested paths..."
redis-cli -h "$HOST" --raw am.save doc2 > /tmp/nested-saved.bin
truncate -s -1 /tmp/nested-saved.bin
redis-cli -h "$HOST" del doc2 > /dev/null
redis-cli -h "$HOST" --raw -x am.load doc2 < /tmp/nested-saved.bin > /dev/null

val=$(redis-cli -h "$HOST" --raw am.gettext doc2 user.profile.name)
assert_equals "$val" "Bob"
val=$(redis-cli -h "$HOST" am.getint doc2 user.profile.age)
assert_equals "$val" "25"
val=$(redis-cli -h "$HOST" am.getdouble doc2 metrics.cpu.usage)
assert_equals "$val" "75.5"
val=$(redis-cli -h "$HOST" am.getbool doc2 flags.features.enabled)
assert_equals "$val" "1"
val=$(redis-cli -h "$HOST" am.getcounter doc2 stats.pageviews)
assert_equals "$val" "150"
echo "   ✓ Nested paths persist and reload correctly"

# Test mixing flat and nested keys
echo "Test 5: Mixed flat and nested keys..."
redis-cli -h "$HOST" del doc5 > /dev/null
redis-cli -h "$HOST" am.new doc5 > /dev/null
redis-cli -h "$HOST" am.puttext doc5 simple "flat value" > /dev/null
redis-cli -h "$HOST" am.puttext doc5 nested.key "nested value" > /dev/null
val1=$(redis-cli -h "$HOST" --raw am.gettext doc5 simple)
val2=$(redis-cli -h "$HOST" --raw am.gettext doc5 nested.key)
assert_equals "$val1" "flat value"
assert_equals "$val2" "nested value"
echo "   ✓ Mixed flat and nested keys work"

rm -f /tmp/nested-saved.bin

echo ""
echo "✅ All nested path tests passed!"
