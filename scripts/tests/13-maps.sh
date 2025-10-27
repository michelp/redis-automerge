#!/usr/bin/env bash
# Test map operations

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Map Operations"

# Test 1: Root level map length
echo "Test 1: Root level map length..."
redis-cli -h "$HOST" del mapdoc1 > /dev/null
redis-cli -h "$HOST" am.new mapdoc1 > /dev/null
# Empty document should have 0 keys
len=$(redis-cli -h "$HOST" am.maplen mapdoc1 "")
assert_equals "$len" "0"
# Add some keys
redis-cli -h "$HOST" am.puttext mapdoc1 name "Alice" > /dev/null
redis-cli -h "$HOST" am.putint mapdoc1 age 30 > /dev/null
redis-cli -h "$HOST" am.putbool mapdoc1 active true > /dev/null
len=$(redis-cli -h "$HOST" am.maplen mapdoc1 "")
assert_equals "$len" "3"
echo "   ✓ Root level map length works"

# Test 2: Nested map length
echo "Test 2: Nested map length..."
redis-cli -h "$HOST" del mapdoc2 > /dev/null
redis-cli -h "$HOST" am.new mapdoc2 > /dev/null
# Create nested structure
redis-cli -h "$HOST" am.puttext mapdoc2 user.name "Bob" > /dev/null
redis-cli -h "$HOST" am.puttext mapdoc2 user.email "bob@example.com" > /dev/null
redis-cli -h "$HOST" am.putint mapdoc2 user.age 25 > /dev/null
# Check nested map length
len=$(redis-cli -h "$HOST" am.maplen mapdoc2 user)
assert_equals "$len" "3"
# Check root length (should have 1 key: "user")
len=$(redis-cli -h "$HOST" am.maplen mapdoc2 "")
assert_equals "$len" "1"
echo "   ✓ Nested map length works"

# Test 3: Deeply nested maps
echo "Test 3: Deeply nested maps..."
redis-cli -h "$HOST" del mapdoc3 > /dev/null
redis-cli -h "$HOST" am.new mapdoc3 > /dev/null
# Create deeply nested structure
redis-cli -h "$HOST" am.puttext mapdoc3 a.b.c.d.key1 "value1" > /dev/null
redis-cli -h "$HOST" am.puttext mapdoc3 a.b.c.d.key2 "value2" > /dev/null
redis-cli -h "$HOST" am.putint mapdoc3 a.b.c.d.key3 100 > /dev/null
redis-cli -h "$HOST" am.putbool mapdoc3 a.b.c.d.key4 true > /dev/null
# Check deeply nested map
len=$(redis-cli -h "$HOST" am.maplen mapdoc3 a.b.c.d)
assert_equals "$len" "4"
# Check intermediate levels
len=$(redis-cli -h "$HOST" am.maplen mapdoc3 a.b.c)
assert_equals "$len" "1"
len=$(redis-cli -h "$HOST" am.maplen mapdoc3 a.b)
assert_equals "$len" "1"
len=$(redis-cli -h "$HOST" am.maplen mapdoc3 a)
assert_equals "$len" "1"
echo "   ✓ Deeply nested maps work"

# Test 4: Mixed maps and lists
echo "Test 4: Mixed maps and lists..."
redis-cli -h "$HOST" del mapdoc4 > /dev/null
redis-cli -h "$HOST" am.new mapdoc4 > /dev/null
# Create mixed structure
redis-cli -h "$HOST" am.puttext mapdoc4 config.name "MyApp" > /dev/null
redis-cli -h "$HOST" am.putint mapdoc4 config.version 1 > /dev/null
redis-cli -h "$HOST" am.createlist mapdoc4 config.features > /dev/null
redis-cli -h "$HOST" am.appendtext mapdoc4 config.features "auth" > /dev/null
redis-cli -h "$HOST" am.appendtext mapdoc4 config.features "api" > /dev/null
# Map length should count all keys including the list
len=$(redis-cli -h "$HOST" am.maplen mapdoc4 config)
assert_equals "$len" "3"
# List length should work separately
listlen=$(redis-cli -h "$HOST" am.listlen mapdoc4 config.features)
assert_equals "$listlen" "2"
echo "   ✓ Mixed maps and lists work"

# Test 5: Non-existent path returns null
echo "Test 5: Non-existent path returns null..."
redis-cli -h "$HOST" del mapdoc5 > /dev/null
redis-cli -h "$HOST" am.new mapdoc5 > /dev/null
redis-cli -h "$HOST" am.puttext mapdoc5 existing "value" > /dev/null
# Try to get length of non-existent path
result=$(redis-cli -h "$HOST" am.maplen mapdoc5 nonexistent)
assert_equals "$result" ""
echo "   ✓ Non-existent path returns null"

# Test 6: Map length after modifications
echo "Test 6: Map length after modifications..."
redis-cli -h "$HOST" del mapdoc6 > /dev/null
redis-cli -h "$HOST" am.new mapdoc6 > /dev/null
# Start with some keys
redis-cli -h "$HOST" am.puttext mapdoc6 data.key1 "value1" > /dev/null
redis-cli -h "$HOST" am.puttext mapdoc6 data.key2 "value2" > /dev/null
len=$(redis-cli -h "$HOST" am.maplen mapdoc6 data)
assert_equals "$len" "2"
# Add more keys
redis-cli -h "$HOST" am.puttext mapdoc6 data.key3 "value3" > /dev/null
redis-cli -h "$HOST" am.putint mapdoc6 data.key4 42 > /dev/null
len=$(redis-cli -h "$HOST" am.maplen mapdoc6 data)
assert_equals "$len" "4"
# Overwrite existing key (should not change count)
redis-cli -h "$HOST" am.puttext mapdoc6 data.key1 "new value" > /dev/null
len=$(redis-cli -h "$HOST" am.maplen mapdoc6 data)
assert_equals "$len" "4"
echo "   ✓ Map length updates correctly after modifications"

# Test 7: JSONPath-style paths
echo "Test 7: JSONPath-style paths with $ prefix..."
redis-cli -h "$HOST" del mapdoc7 > /dev/null
redis-cli -h "$HOST" am.new mapdoc7 > /dev/null
redis-cli -h "$HOST" am.puttext mapdoc7 '$.settings.theme' "dark" > /dev/null
redis-cli -h "$HOST" am.puttext mapdoc7 '$.settings.lang' "en" > /dev/null
redis-cli -h "$HOST" am.putint mapdoc7 '$.settings.timeout' 30 > /dev/null
# Check with $ prefix
len=$(redis-cli -h "$HOST" am.maplen mapdoc7 '$.settings')
assert_equals "$len" "3"
# Check without $ prefix
len=$(redis-cli -h "$HOST" am.maplen mapdoc7 settings)
assert_equals "$len" "3"
echo "   ✓ JSONPath-style $ prefix works"

# Test 8: Map persistence
echo "Test 8: Map persistence..."
redis-cli -h "$HOST" del mapdoc8 > /dev/null
redis-cli -h "$HOST" am.new mapdoc8 > /dev/null
redis-cli -h "$HOST" am.puttext mapdoc8 persistent.key1 "value1" > /dev/null
redis-cli -h "$HOST" am.puttext mapdoc8 persistent.key2 "value2" > /dev/null
redis-cli -h "$HOST" am.putint mapdoc8 persistent.key3 123 > /dev/null
len_before=$(redis-cli -h "$HOST" am.maplen mapdoc8 persistent)
# Save and reload
redis-cli -h "$HOST" --raw am.save mapdoc8 > /tmp/map-saved.bin
truncate -s -1 /tmp/map-saved.bin
redis-cli -h "$HOST" del mapdoc8 > /dev/null
redis-cli -h "$HOST" --raw -x am.load mapdoc8 < /tmp/map-saved.bin > /dev/null
len_after=$(redis-cli -h "$HOST" am.maplen mapdoc8 persistent)
assert_equals "$len_before" "3"
assert_equals "$len_after" "3"
# Verify values persisted correctly
val=$(redis-cli -h "$HOST" --raw am.gettext mapdoc8 persistent.key1)
assert_equals "$val" "value1"
echo "   ✓ Map length persists correctly"

# Test 9: Map with all data types
echo "Test 9: Map with all data types..."
redis-cli -h "$HOST" del mapdoc9 > /dev/null
redis-cli -h "$HOST" am.new mapdoc9 > /dev/null
# Add different types
redis-cli -h "$HOST" am.puttext mapdoc9 types.text "hello" > /dev/null
redis-cli -h "$HOST" am.putint mapdoc9 types.integer 42 > /dev/null
redis-cli -h "$HOST" am.putdouble mapdoc9 types.double 3.14 > /dev/null
redis-cli -h "$HOST" am.putbool mapdoc9 types.boolean true > /dev/null
redis-cli -h "$HOST" am.putcounter mapdoc9 types.counter 10 > /dev/null
redis-cli -h "$HOST" am.createlist mapdoc9 types.list > /dev/null
# Should count all 6 keys
len=$(redis-cli -h "$HOST" am.maplen mapdoc9 types)
assert_equals "$len" "6"
echo "   ✓ Map with all data types works"

# Test 10: Empty nested maps
echo "Test 10: Empty nested maps..."
redis-cli -h "$HOST" del mapdoc10 > /dev/null
redis-cli -h "$HOST" am.new mapdoc10 > /dev/null
# Create nested structure, then check an intermediate empty map
redis-cli -h "$HOST" am.puttext mapdoc10 outer.inner.value "test" > /dev/null
# "inner" map should have 1 key
len=$(redis-cli -h "$HOST" am.maplen mapdoc10 outer.inner)
assert_equals "$len" "1"
# Now create a sibling empty path by setting another nested value
redis-cli -h "$HOST" am.puttext mapdoc10 outer.another.deep.value "test2" > /dev/null
# "outer" should now have 2 keys: "inner" and "another"
len=$(redis-cli -h "$HOST" am.maplen mapdoc10 outer)
assert_equals "$len" "2"
# "another" should have 1 key: "deep"
len=$(redis-cli -h "$HOST" am.maplen mapdoc10 outer.another)
assert_equals "$len" "1"
echo "   ✓ Empty nested maps work"

rm -f /tmp/map-saved.bin

echo ""
echo "✅ All map operation tests passed!"
