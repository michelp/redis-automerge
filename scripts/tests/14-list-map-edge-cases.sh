#!/usr/bin/env bash
# Test edge cases for list and map operations

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "List and Map Edge Cases"

# Test 1: Empty list
echo "Test 1: Empty list creation and length..."
redis-cli -h "$HOST" del edgedoc1 > /dev/null
redis-cli -h "$HOST" am.new edgedoc1 > /dev/null
redis-cli -h "$HOST" am.createlist edgedoc1 empty > /dev/null
len=$(redis-cli -h "$HOST" am.listlen edgedoc1 empty)
assert_equals "$len" "0"
echo "   ✓ Empty list has length 0"

# Test 2: Out of bounds array access
echo "Test 2: Out of bounds array access..."
redis-cli -h "$HOST" del edgedoc2 > /dev/null
redis-cli -h "$HOST" am.new edgedoc2 > /dev/null
redis-cli -h "$HOST" am.createlist edgedoc2 items > /dev/null
redis-cli -h "$HOST" am.appendtext edgedoc2 items "first" > /dev/null
# Access beyond list length should return null
result=$(redis-cli -h "$HOST" am.gettext edgedoc2 'items[5]')
assert_equals "$result" ""
echo "   ✓ Out of bounds access returns null"

# Test 3: Getting bool and double from lists
echo "Test 3: Getting bool and double from lists..."
redis-cli -h "$HOST" del edgedoc3 > /dev/null
redis-cli -h "$HOST" am.new edgedoc3 > /dev/null
redis-cli -h "$HOST" am.createlist edgedoc3 bools > /dev/null
redis-cli -h "$HOST" am.appendbool edgedoc3 bools true > /dev/null
redis-cli -h "$HOST" am.appendbool edgedoc3 bools false > /dev/null
val1=$(redis-cli -h "$HOST" am.getbool edgedoc3 'bools[0]')
val2=$(redis-cli -h "$HOST" am.getbool edgedoc3 'bools[1]')
assert_equals "$val1" "1"
assert_equals "$val2" "0"
# Test doubles
redis-cli -h "$HOST" am.createlist edgedoc3 doubles > /dev/null
redis-cli -h "$HOST" am.appenddouble edgedoc3 doubles 3.14 > /dev/null
redis-cli -h "$HOST" am.appenddouble edgedoc3 doubles 2.71 > /dev/null
val1=$(redis-cli -h "$HOST" am.getdouble edgedoc3 'doubles[0]')
val2=$(redis-cli -h "$HOST" am.getdouble edgedoc3 'doubles[1]')
assert_equals "$val1" "3.14"
assert_equals "$val2" "2.71"
echo "   ✓ Getting bool and double from lists works"

# Test 4: Mixed types in same list
echo "Test 4: Mixed types in same list..."
redis-cli -h "$HOST" del edgedoc4 > /dev/null
redis-cli -h "$HOST" am.new edgedoc4 > /dev/null
redis-cli -h "$HOST" am.createlist edgedoc4 mixed > /dev/null
redis-cli -h "$HOST" am.appendtext edgedoc4 mixed "hello" > /dev/null
redis-cli -h "$HOST" am.appendint edgedoc4 mixed 42 > /dev/null
redis-cli -h "$HOST" am.appendbool edgedoc4 mixed true > /dev/null
redis-cli -h "$HOST" am.appenddouble edgedoc4 mixed 3.14 > /dev/null
# Verify each type
val1=$(redis-cli -h "$HOST" --raw am.gettext edgedoc4 'mixed[0]')
val2=$(redis-cli -h "$HOST" am.getint edgedoc4 'mixed[1]')
val3=$(redis-cli -h "$HOST" am.getbool edgedoc4 'mixed[2]')
val4=$(redis-cli -h "$HOST" am.getdouble edgedoc4 'mixed[3]')
len=$(redis-cli -h "$HOST" am.listlen edgedoc4 mixed)
assert_equals "$val1" "hello"
assert_equals "$val2" "42"
assert_equals "$val3" "1"
assert_equals "$val4" "3.14"
assert_equals "$len" "4"
echo "   ✓ Mixed types in same list work"

# Test 5: List length of non-existent list
echo "Test 5: List length of non-existent list..."
redis-cli -h "$HOST" del edgedoc5 > /dev/null
redis-cli -h "$HOST" am.new edgedoc5 > /dev/null
result=$(redis-cli -h "$HOST" am.listlen edgedoc5 nonexistent)
assert_equals "$result" ""
echo "   ✓ Non-existent list returns null for length"

# Test 6: MAPLEN on scalar value
echo "Test 6: MAPLEN on scalar value..."
redis-cli -h "$HOST" del edgedoc6 > /dev/null
redis-cli -h "$HOST" am.new edgedoc6 > /dev/null
redis-cli -h "$HOST" am.puttext edgedoc6 name "Alice" > /dev/null
# MAPLEN on a scalar should return null (scalars have no keys)
result=$(redis-cli -h "$HOST" am.maplen edgedoc6 name)
assert_equals "$result" ""
echo "   ✓ MAPLEN on scalar returns null"

# Test 7: MAPLEN on list
echo "Test 7: MAPLEN on list..."
redis-cli -h "$HOST" del edgedoc7 > /dev/null
redis-cli -h "$HOST" am.new edgedoc7 > /dev/null
redis-cli -h "$HOST" am.createlist edgedoc7 items > /dev/null
redis-cli -h "$HOST" am.appendtext edgedoc7 items "a" > /dev/null
redis-cli -h "$HOST" am.appendtext edgedoc7 items "b" > /dev/null
# MAPLEN on a list returns the number of indices (same as list length)
result=$(redis-cli -h "$HOST" am.maplen edgedoc7 items)
assert_equals "$result" "2"
echo "   ✓ MAPLEN on list returns number of indices (same as LISTLEN)"

# Test 8: LISTLEN on map
echo "Test 8: LISTLEN on map..."
redis-cli -h "$HOST" del edgedoc8 > /dev/null
redis-cli -h "$HOST" am.new edgedoc8 > /dev/null
redis-cli -h "$HOST" am.puttext edgedoc8 user.name "Bob" > /dev/null
redis-cli -h "$HOST" am.putint edgedoc8 user.age 30 > /dev/null
# LISTLEN on a map returns the number of keys (same as MAPLEN)
# This is because in Automerge, both maps and lists support length operation
result=$(redis-cli -h "$HOST" am.listlen edgedoc8 user)
assert_equals "$result" "2"
echo "   ✓ LISTLEN on map returns number of keys (same as MAPLEN)"

# Test 9: Accessing nested map properties within simple list items
echo "Test 9: List with nested map structure..."
redis-cli -h "$HOST" del edgedoc9 > /dev/null
redis-cli -h "$HOST" am.new edgedoc9 > /dev/null
# Create a structure with arrays and nested objects (without arrays of objects)
redis-cli -h "$HOST" am.fromjson edgedoc9 '{"project":{"name":"MyProject","versions":["v1.0","v2.0","v3.0"]}}' > /dev/null
# Verify nested structure
proj_name=$(redis-cli -h "$HOST" --raw am.gettext edgedoc9 project.name)
ver0=$(redis-cli -h "$HOST" --raw am.gettext edgedoc9 'project.versions[0]')
ver1=$(redis-cli -h "$HOST" --raw am.gettext edgedoc9 'project.versions[1]')
ver2=$(redis-cli -h "$HOST" --raw am.gettext edgedoc9 'project.versions[2]')
assert_equals "$proj_name" "MyProject"
assert_equals "$ver0" "v1.0"
assert_equals "$ver1" "v2.0"
assert_equals "$ver2" "v3.0"
# Check lengths
maplen=$(redis-cli -h "$HOST" am.maplen edgedoc9 project)
listlen=$(redis-cli -h "$HOST" am.listlen edgedoc9 project.versions)
assert_equals "$maplen" "2"
assert_equals "$listlen" "3"
# Modify values
redis-cli -h "$HOST" am.puttext edgedoc9 project.name "UpdatedProject" > /dev/null
updated_name=$(redis-cli -h "$HOST" --raw am.gettext edgedoc9 project.name)
assert_equals "$updated_name" "UpdatedProject"
echo "   ✓ Nested maps with lists work correctly"

# Test 10: Complex nested structure with mixed types
echo "Test 10: Complex nested structure persistence..."
redis-cli -h "$HOST" del edgedoc10 > /dev/null
# Create a complex structure using FROMJSON (avoiding arrays of objects)
redis-cli -h "$HOST" am.fromjson edgedoc10 '{"config":{"name":"MyApp","enabled":true,"timeout":30.5,"tags":["production","backend"]}}' > /dev/null
# Save and reload
redis-cli -h "$HOST" --raw am.save edgedoc10 > /tmp/edge-saved.bin
truncate -s -1 /tmp/edge-saved.bin
redis-cli -h "$HOST" del edgedoc10 > /dev/null
redis-cli -h "$HOST" --raw -x am.load edgedoc10 < /tmp/edge-saved.bin > /dev/null
# Verify complex structure persisted
name=$(redis-cli -h "$HOST" --raw am.gettext edgedoc10 config.name)
enabled=$(redis-cli -h "$HOST" am.getbool edgedoc10 config.enabled)
timeout=$(redis-cli -h "$HOST" am.getdouble edgedoc10 config.timeout)
tag0=$(redis-cli -h "$HOST" --raw am.gettext edgedoc10 'config.tags[0]')
tag1=$(redis-cli -h "$HOST" --raw am.gettext edgedoc10 'config.tags[1]')
assert_equals "$name" "MyApp"
assert_equals "$enabled" "1"
assert_equals "$timeout" "30.5"
assert_equals "$tag0" "production"
assert_equals "$tag1" "backend"
# Check lengths
maplen=$(redis-cli -h "$HOST" am.maplen edgedoc10 config)
listlen=$(redis-cli -h "$HOST" am.listlen edgedoc10 config.tags)
assert_equals "$maplen" "4"
assert_equals "$listlen" "2"
echo "   ✓ Complex nested structure persists correctly"

# Test 11: JSON export of complex nested structure
echo "Test 11: JSON export of complex nested structure..."
redis-cli -h "$HOST" del edgedoc11 > /dev/null
# Build complex structure using FROMJSON
redis-cli -h "$HOST" am.fromjson edgedoc11 '{"items":[1,2,3],"nested":{"values":[true,false],"meta":{"count":42}}}' > /dev/null
# Export to JSON
json=$(redis-cli -h "$HOST" --raw am.tojson edgedoc11)
# Check that JSON contains expected structure
if [ -z "$json" ] || [ "$json" = "{}" ]; then
    echo "   ✗ JSON export failed or returned empty"
    exit 1
fi
# Use jq to validate JSON structure
echo "$json" | jq -e '.items[0] == 1' > /dev/null
echo "$json" | jq -e '.items[1] == 2' > /dev/null
echo "$json" | jq -e '.nested.values[0] == true' > /dev/null
echo "$json" | jq -e '.nested.meta.count == 42' > /dev/null
echo "   ✓ JSON export of complex nested structure works"

rm -f /tmp/edge-saved.bin

echo ""
echo "✅ All list and map edge case tests passed!"
