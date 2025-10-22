#!/usr/bin/env bash
# Test change management: AM.CHANGES, AM.NUMCHANGES, AM.APPLY

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Change Management"

# Test AM.CHANGES command
echo "Test 1: AM.CHANGES with no changes..."
redis-cli -h "$HOST" del changes_test1 > /dev/null
redis-cli -h "$HOST" am.new changes_test1 > /dev/null
# Get all changes from empty document (should return empty array)
changes=$(redis-cli -h "$HOST" am.changes changes_test1)
assert_equals "$changes" ""
echo "   ✓ AM.CHANGES returns empty array for new document"

echo "Test 2: AM.NUMCHANGES with single change..."
redis-cli -h "$HOST" del changes_test2 > /dev/null
redis-cli -h "$HOST" am.new changes_test2 > /dev/null
redis-cli -h "$HOST" am.puttext changes_test2 name "Alice" > /dev/null
# Get all changes (should return 1 change)
num_changes=$(redis-cli -h "$HOST" am.numchanges changes_test2)
assert_equals "$num_changes" "1"
echo "   ✓ AM.NUMCHANGES returns single change after one operation"

echo "Test 3: AM.NUMCHANGES with multiple changes..."
redis-cli -h "$HOST" del changes_test3 > /dev/null
redis-cli -h "$HOST" am.new changes_test3 > /dev/null
redis-cli -h "$HOST" am.puttext changes_test3 name "Bob" > /dev/null
redis-cli -h "$HOST" am.putint changes_test3 age 25 > /dev/null
redis-cli -h "$HOST" am.putbool changes_test3 active true > /dev/null
# Get all changes (should return 3 changes)
num_changes=$(redis-cli -h "$HOST" am.numchanges changes_test3)
assert_equals "$num_changes" "3"
echo "   ✓ AM.NUMCHANGES returns all changes after multiple operations"

echo "Test 4: AM.CHANGES returns binary data..."
redis-cli -h "$HOST" del changes_test4 > /dev/null
redis-cli -h "$HOST" am.new changes_test4 > /dev/null
redis-cli -h "$HOST" am.puttext changes_test4 field "value" > /dev/null
# Get changes and verify we got binary data back
redis-cli -h "$HOST" am.changes changes_test4 > /tmp/changes_test4.bin
if [ -s /tmp/changes_test4.bin ]; then
    echo "   ✓ AM.CHANGES returns binary change data"
else
    echo "   ✗ AM.CHANGES did not return expected data"
    exit 1
fi
rm -f /tmp/changes_test4.bin

echo "Test 5: AM.CHANGES with persistence..."
redis-cli -h "$HOST" del changes_test5 > /dev/null
redis-cli -h "$HOST" am.new changes_test5 > /dev/null
redis-cli -h "$HOST" am.puttext changes_test5 data "initial" > /dev/null
redis-cli -h "$HOST" am.puttext changes_test5 data "updated" > /dev/null
# Get changes count before save
changes_before=$(redis-cli -h "$HOST" am.numchanges changes_test5)
# Save and reload
redis-cli -h "$HOST" --raw am.save changes_test5 > /tmp/changes_test5.bin
truncate -s -1 /tmp/changes_test5.bin
redis-cli -h "$HOST" del changes_test5 > /dev/null
redis-cli -h "$HOST" --raw -x am.load changes_test5 < /tmp/changes_test5.bin > /dev/null
# Get changes count after reload
changes_after=$(redis-cli -h "$HOST" am.numchanges changes_test5)
assert_equals "$changes_before" "$changes_after"
echo "   ✓ AM.CHANGES works after save/load (both returned $changes_before changes)"
rm -f /tmp/changes_test5.bin

echo "Test 6: AM.NUMCHANGES with list operations..."
redis-cli -h "$HOST" del changes_test6 > /dev/null
redis-cli -h "$HOST" am.new changes_test6 > /dev/null
redis-cli -h "$HOST" am.createlist changes_test6 items > /dev/null
redis-cli -h "$HOST" am.appendtext changes_test6 items "first" > /dev/null
redis-cli -h "$HOST" am.appendtext changes_test6 items "second" > /dev/null
# Get all changes (should return 3: createlist + 2 appends but actually 4 because createlist creates a change)
num_changes=$(redis-cli -h "$HOST" am.numchanges changes_test6)
assert_equals "$num_changes" "3"
echo "   ✓ AM.NUMCHANGES tracks list operations correctly"

echo "Test 7: AM.NUMCHANGES with nested paths..."
redis-cli -h "$HOST" del changes_test7 > /dev/null
redis-cli -h "$HOST" am.new changes_test7 > /dev/null
redis-cli -h "$HOST" am.puttext changes_test7 user.name "Carol" > /dev/null
redis-cli -h "$HOST" am.putint changes_test7 user.age 30 > /dev/null
redis-cli -h "$HOST" am.puttext changes_test7 user.profile.bio "Developer" > /dev/null
# Get all changes (should return 3)
num_changes=$(redis-cli -h "$HOST" am.numchanges changes_test7)
assert_equals "$num_changes" "3"
echo "   ✓ AM.NUMCHANGES tracks nested path operations correctly"

echo ""
echo "✅ All change management tests passed!"
