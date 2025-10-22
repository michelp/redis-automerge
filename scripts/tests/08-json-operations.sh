#!/usr/bin/env bash
# Test JSON import/export operations (AM.TOJSON and AM.FROMJSON)

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "JSON Operations (TOJSON & FROMJSON)"

# AM.TOJSON tests
echo "Test 1: AM.TOJSON with empty document..."
redis-cli -h "$HOST" del json_test1 > /dev/null
redis-cli -h "$HOST" am.new json_test1 > /dev/null
json=$(redis-cli -h "$HOST" --raw am.tojson json_test1)
assert_equals "$json" "{}"
echo "   ✓ AM.TOJSON returns {} for empty document"

echo "Test 2: AM.TOJSON with simple types..."
redis-cli -h "$HOST" del json_test2 > /dev/null
redis-cli -h "$HOST" am.new json_test2 > /dev/null
redis-cli -h "$HOST" am.puttext json_test2 name "Alice" > /dev/null
redis-cli -h "$HOST" am.putint json_test2 age 30 > /dev/null
redis-cli -h "$HOST" am.putdouble json_test2 score 95.5 > /dev/null
redis-cli -h "$HOST" am.putbool json_test2 active true > /dev/null
json=$(redis-cli -h "$HOST" --raw am.tojson json_test2)
# Parse and verify (using jq for JSON parsing)
name=$(echo "$json" | jq -r '.name')
age=$(echo "$json" | jq -r '.age')
score=$(echo "$json" | jq -r '.score')
active=$(echo "$json" | jq -r '.active')
assert_equals "$name" "Alice"
assert_equals "$age" "30"
assert_equals "$score" "95.5"
assert_equals "$active" "true"
echo "   ✓ AM.TOJSON returns correct JSON for simple types"

echo "Test 3: AM.TOJSON with nested objects..."
redis-cli -h "$HOST" del json_test3 > /dev/null
redis-cli -h "$HOST" am.new json_test3 > /dev/null
redis-cli -h "$HOST" am.puttext json_test3 user.profile.name "Bob" > /dev/null
redis-cli -h "$HOST" am.putint json_test3 user.profile.age 25 > /dev/null
redis-cli -h "$HOST" am.puttext json_test3 user.email "bob@example.com" > /dev/null
json=$(redis-cli -h "$HOST" --raw am.tojson json_test3)
profile_name=$(echo "$json" | jq -r '.user.profile.name')
profile_age=$(echo "$json" | jq -r '.user.profile.age')
email=$(echo "$json" | jq -r '.user.email')
assert_equals "$profile_name" "Bob"
assert_equals "$profile_age" "25"
assert_equals "$email" "bob@example.com"
echo "   ✓ AM.TOJSON returns correct JSON for nested objects"

echo "Test 4: AM.TOJSON with lists..."
redis-cli -h "$HOST" del json_test4 > /dev/null
redis-cli -h "$HOST" am.new json_test4 > /dev/null
redis-cli -h "$HOST" am.createlist json_test4 tags > /dev/null
redis-cli -h "$HOST" am.appendtext json_test4 tags "redis" > /dev/null
redis-cli -h "$HOST" am.appendtext json_test4 tags "crdt" > /dev/null
redis-cli -h "$HOST" am.appendtext json_test4 tags "rust" > /dev/null
json=$(redis-cli -h "$HOST" --raw am.tojson json_test4)
tag0=$(echo "$json" | jq -r '.tags[0]')
tag1=$(echo "$json" | jq -r '.tags[1]')
tag2=$(echo "$json" | jq -r '.tags[2]')
tag_count=$(echo "$json" | jq -r '.tags | length')
assert_equals "$tag0" "redis"
assert_equals "$tag1" "crdt"
assert_equals "$tag2" "rust"
assert_equals "$tag_count" "3"
echo "   ✓ AM.TOJSON returns correct JSON for lists"

echo "Test 5: AM.TOJSON with mixed list types..."
redis-cli -h "$HOST" del json_test5 > /dev/null
redis-cli -h "$HOST" am.new json_test5 > /dev/null
redis-cli -h "$HOST" am.createlist json_test5 mixed > /dev/null
redis-cli -h "$HOST" am.appendtext json_test5 mixed "text" > /dev/null
redis-cli -h "$HOST" am.appendint json_test5 mixed 42 > /dev/null
redis-cli -h "$HOST" am.appenddouble json_test5 mixed 3.14 > /dev/null
redis-cli -h "$HOST" am.appendbool json_test5 mixed true > /dev/null
json=$(redis-cli -h "$HOST" --raw am.tojson json_test5)
item0=$(echo "$json" | jq -r '.mixed[0]')
item1=$(echo "$json" | jq -r '.mixed[1]')
item2=$(echo "$json" | jq -r '.mixed[2]')
item3=$(echo "$json" | jq -r '.mixed[3]')
assert_equals "$item0" "text"
assert_equals "$item1" "42"
assert_equals "$item2" "3.14"
assert_equals "$item3" "true"
echo "   ✓ AM.TOJSON returns correct JSON for mixed list types"

echo "Test 6: AM.TOJSON with pretty formatting..."
redis-cli -h "$HOST" del json_test6 > /dev/null
redis-cli -h "$HOST" am.new json_test6 > /dev/null
redis-cli -h "$HOST" am.puttext json_test6 name "Alice" > /dev/null
redis-cli -h "$HOST" am.putint json_test6 age 30 > /dev/null
# Get compact JSON (default)
compact=$(redis-cli -h "$HOST" --raw am.tojson json_test6)
# Get pretty JSON
pretty=$(redis-cli -h "$HOST" --raw am.tojson json_test6 true)
# Compact should not have newlines (except possibly trailing)
compact_lines=$(echo "$compact" | grep -c . || echo 0)
# Pretty should have multiple lines
pretty_lines=$(echo "$pretty" | grep -c . || echo 0)
if [ "$compact_lines" -eq 1 ] && [ "$pretty_lines" -gt 1 ]; then
    echo "   ✓ AM.TOJSON pretty formatting works"
else
    echo "   ✗ Pretty formatting didn't work as expected"
    echo "      Compact lines: $compact_lines (expected 1)"
    echo "      Pretty lines: $pretty_lines (expected > 1)"
    exit 1
fi

echo "Test 7: AM.TOJSON with complex structure..."
redis-cli -h "$HOST" del json_test7 > /dev/null
redis-cli -h "$HOST" am.new json_test7 > /dev/null
redis-cli -h "$HOST" am.puttext json_test7 user.name "Alice" > /dev/null
redis-cli -h "$HOST" am.putint json_test7 user.age 30 > /dev/null
redis-cli -h "$HOST" am.createlist json_test7 user.hobbies > /dev/null
redis-cli -h "$HOST" am.appendtext json_test7 user.hobbies "reading" > /dev/null
redis-cli -h "$HOST" am.appendtext json_test7 user.hobbies "coding" > /dev/null
redis-cli -h "$HOST" am.puttext json_test7 config.database.host "localhost" > /dev/null
redis-cli -h "$HOST" am.putint json_test7 config.database.port 5432 > /dev/null
json=$(redis-cli -h "$HOST" --raw am.tojson json_test7)
user_name=$(echo "$json" | jq -r '.user.name')
user_age=$(echo "$json" | jq -r '.user.age')
hobby0=$(echo "$json" | jq -r '.user.hobbies[0]')
hobby1=$(echo "$json" | jq -r '.user.hobbies[1]')
db_host=$(echo "$json" | jq -r '.config.database.host')
db_port=$(echo "$json" | jq -r '.config.database.port')
assert_equals "$user_name" "Alice"
assert_equals "$user_age" "30"
assert_equals "$hobby0" "reading"
assert_equals "$hobby1" "coding"
assert_equals "$db_host" "localhost"
assert_equals "$db_port" "5432"
echo "   ✓ AM.TOJSON returns correct JSON for complex structure"

echo "Test 8: AM.TOJSON with timestamps as ISO 8601..."
redis-cli -h "$HOST" del json_test_ts > /dev/null
redis-cli -h "$HOST" am.new json_test_ts > /dev/null
# 1704067200000 = 2024-01-01 00:00:00 UTC
redis-cli -h "$HOST" am.puttimestamp json_test_ts created_at 1704067200000 > /dev/null
redis-cli -h "$HOST" am.puttext json_test_ts name "Event" > /dev/null
json=$(redis-cli -h "$HOST" --raw am.tojson json_test_ts)
timestamp=$(echo "$json" | jq -r '.created_at')
# Check that it starts with 2024-01-01T00:00:00 (ISO 8601 format)
if [[ "$timestamp" == 2024-01-01T00:00:00* ]]; then
    echo "   ✓ AM.TOJSON returns ISO 8601 string for timestamps: $timestamp"
else
    echo "   ✗ AM.TOJSON did not return ISO 8601 timestamp, got: $timestamp"
    exit 1
fi

echo "Test 9: AM.TOJSON persistence roundtrip..."
redis-cli -h "$HOST" del json_test8 > /dev/null
redis-cli -h "$HOST" am.new json_test8 > /dev/null
redis-cli -h "$HOST" am.puttext json_test8 name "Charlie" > /dev/null
redis-cli -h "$HOST" am.putint json_test8 count 100 > /dev/null
redis-cli -h "$HOST" am.createlist json_test8 items > /dev/null
redis-cli -h "$HOST" am.appendtext json_test8 items "a" > /dev/null
redis-cli -h "$HOST" am.appendtext json_test8 items "b" > /dev/null
# Get JSON before save
json_before=$(redis-cli -h "$HOST" --raw am.tojson json_test8)
name_before=$(echo "$json_before" | jq -r '.name')
count_before=$(echo "$json_before" | jq -r '.count')
item0_before=$(echo "$json_before" | jq -r '.items[0]')
item1_before=$(echo "$json_before" | jq -r '.items[1]')
# Save and reload
redis-cli -h "$HOST" --raw am.save json_test8 > /tmp/json_test8.bin
truncate -s -1 /tmp/json_test8.bin
redis-cli -h "$HOST" del json_test8 > /dev/null
redis-cli -h "$HOST" --raw -x am.load json_test8 < /tmp/json_test8.bin > /dev/null
# Get JSON after reload
json_after=$(redis-cli -h "$HOST" --raw am.tojson json_test8)
name_after=$(echo "$json_after" | jq -r '.name')
count_after=$(echo "$json_after" | jq -r '.count')
item0_after=$(echo "$json_after" | jq -r '.items[0]')
item1_after=$(echo "$json_after" | jq -r '.items[1]')
assert_equals "$name_before" "$name_after"
assert_equals "$count_before" "$count_after"
assert_equals "$item0_before" "$item0_after"
assert_equals "$item1_before" "$item1_after"
echo "   ✓ AM.TOJSON works correctly after save/load"
rm -f /tmp/json_test8.bin

# AM.FROMJSON tests
echo "Test 10: AM.FROMJSON with simple types..."
redis-cli -h "$HOST" del fromjson_test1 > /dev/null
json='{"name":"Alice","age":30,"score":95.5,"active":true}'
echo "$json" | redis-cli -h "$HOST" -x am.fromjson fromjson_test1
# Verify values were set correctly
name=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test1 name)
age=$(redis-cli -h "$HOST" am.getint fromjson_test1 age)
score=$(redis-cli -h "$HOST" am.getdouble fromjson_test1 score)
active=$(redis-cli -h "$HOST" am.getbool fromjson_test1 active)
assert_equals "$name" "Alice"
assert_equals "$age" "30"
assert_equals "$score" "95.5"
assert_equals "$active" "1"
echo "   ✓ AM.FROMJSON simple types work"

echo "Test 11: AM.FROMJSON with nested objects..."
redis-cli -h "$HOST" del fromjson_test2 > /dev/null
json='{"user":{"profile":{"name":"Bob","age":25},"email":"bob@example.com"}}'
echo "$json" | redis-cli -h "$HOST" -x am.fromjson fromjson_test2
# Verify nested values
profile_name=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test2 user.profile.name)
profile_age=$(redis-cli -h "$HOST" am.getint fromjson_test2 user.profile.age)
email=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test2 user.email)
assert_equals "$profile_name" "Bob"
assert_equals "$profile_age" "25"
assert_equals "$email" "bob@example.com"
echo "   ✓ AM.FROMJSON nested objects work"

echo "Test 12: AM.FROMJSON with arrays..."
redis-cli -h "$HOST" del fromjson_test3 > /dev/null
json='{"tags":["redis","crdt","rust"]}'
echo "$json" | redis-cli -h "$HOST" -x am.fromjson fromjson_test3
# Verify array was converted to list
list_len=$(redis-cli -h "$HOST" am.listlen fromjson_test3 tags)
tag0=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test3 'tags[0]')
tag1=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test3 'tags[1]')
tag2=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test3 'tags[2]')
assert_equals "$list_len" "3"
assert_equals "$tag0" "redis"
assert_equals "$tag1" "crdt"
assert_equals "$tag2" "rust"
echo "   ✓ AM.FROMJSON arrays work"

echo "Test 13: AM.FROMJSON with mixed list types..."
redis-cli -h "$HOST" del fromjson_test4 > /dev/null
json='{"mixed":["text",42,3.14,true]}'
echo "$json" | redis-cli -h "$HOST" -x am.fromjson fromjson_test4
# Verify mixed types in list
item0=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test4 'mixed[0]')
item1=$(redis-cli -h "$HOST" am.getint fromjson_test4 'mixed[1]')
item2=$(redis-cli -h "$HOST" am.getdouble fromjson_test4 'mixed[2]')
item3=$(redis-cli -h "$HOST" am.getbool fromjson_test4 'mixed[3]')
assert_equals "$item0" "text"
assert_equals "$item1" "42"
assert_equals "$item2" "3.14"
assert_equals "$item3" "1"
echo "   ✓ AM.FROMJSON mixed list types work"

echo "Test 14: AM.FROMJSON with complex structure..."
redis-cli -h "$HOST" del fromjson_test5 > /dev/null
json='{"user":{"name":"Alice","age":30,"hobbies":["reading","coding"]},"config":{"database":{"host":"localhost","port":5432}}}'
echo "$json" | redis-cli -h "$HOST" -x am.fromjson fromjson_test5
# Verify complex structure
user_name=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test5 user.name)
user_age=$(redis-cli -h "$HOST" am.getint fromjson_test5 user.age)
hobby0=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test5 'user.hobbies[0]')
hobby1=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test5 'user.hobbies[1]')
db_host=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test5 config.database.host)
db_port=$(redis-cli -h "$HOST" am.getint fromjson_test5 config.database.port)
assert_equals "$user_name" "Alice"
assert_equals "$user_age" "30"
assert_equals "$hobby0" "reading"
assert_equals "$hobby1" "coding"
assert_equals "$db_host" "localhost"
assert_equals "$db_port" "5432"
echo "   ✓ AM.FROMJSON complex structure works"

echo "Test 15: AM.FROMJSON to AM.TOJSON roundtrip..."
redis-cli -h "$HOST" del fromjson_test6 > /dev/null
original_json='{"name":"Alice","age":30,"tags":["rust","redis"]}'
echo "$original_json" | redis-cli -h "$HOST" -x am.fromjson fromjson_test6
# Export back to JSON
exported_json=$(redis-cli -h "$HOST" --raw am.tojson fromjson_test6)
# Parse both and compare values
name_original=$(echo "$original_json" | jq -r '.name')
age_original=$(echo "$original_json" | jq -r '.age')
tag0_original=$(echo "$original_json" | jq -r '.tags[0]')
name_exported=$(echo "$exported_json" | jq -r '.name')
age_exported=$(echo "$exported_json" | jq -r '.age')
tag0_exported=$(echo "$exported_json" | jq -r '.tags[0]')
assert_equals "$name_original" "$name_exported"
assert_equals "$age_original" "$age_exported"
assert_equals "$tag0_original" "$tag0_exported"
echo "   ✓ AM.FROMJSON to AM.TOJSON roundtrip works"

echo "Test 16: AM.FROMJSON persistence..."
redis-cli -h "$HOST" del fromjson_test7 > /dev/null
json='{"title":"Document","count":100,"items":["a","b","c"]}'
echo "$json" | redis-cli -h "$HOST" -x am.fromjson fromjson_test7
# Save and reload
redis-cli -h "$HOST" --raw am.save fromjson_test7 > /tmp/fromjson_test7.bin
truncate -s -1 /tmp/fromjson_test7.bin
redis-cli -h "$HOST" del fromjson_test7 > /dev/null
redis-cli -h "$HOST" --raw -x am.load fromjson_test7 < /tmp/fromjson_test7.bin > /dev/null
# Verify values after reload
title=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test7 title)
count=$(redis-cli -h "$HOST" am.getint fromjson_test7 count)
item0=$(redis-cli -h "$HOST" --raw am.gettext fromjson_test7 'items[0]')
assert_equals "$title" "Document"
assert_equals "$count" "100"
assert_equals "$item0" "a"
echo "   ✓ AM.FROMJSON persistence works"
rm -f /tmp/fromjson_test7.bin

echo "Test 17: AM.FROMJSON with empty object..."
redis-cli -h "$HOST" del fromjson_test8 > /dev/null
json='{}'
echo "$json" | redis-cli -h "$HOST" -x am.fromjson fromjson_test8
# Should create empty document
exported=$(redis-cli -h "$HOST" --raw am.tojson fromjson_test8)
assert_equals "$exported" "{}"
echo "   ✓ AM.FROMJSON empty object works"

echo ""
echo "✅ All JSON operation tests passed!"
