#!/usr/bin/env bash
# Test AM.GETDIFF command - document state comparison

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Document Diff Tests"

# Ensure jq is available for JSON parsing
if ! command -v jq &> /dev/null; then
    echo "Installing jq..."
    apt-get update -qq > /dev/null 2>&1 && apt-get install -y -qq jq > /dev/null 2>&1
fi

echo "Test 1: Diff from empty to current state..."
redis-cli -h "$HOST" del diff_test1 > /dev/null
redis-cli -h "$HOST" am.new diff_test1 > /dev/null
redis-cli -h "$HOST" am.puttext diff_test1 name "Alice" > /dev/null
redis-cli -h "$HOST" am.putint diff_test1 age 30 > /dev/null

# Get diff from empty (BEFORE) to current (AFTER)
result=$(redis-cli -h "$HOST" am.getdiff diff_test1 BEFORE AFTER 2>&1)

# Verify result is not an error
if echo "$result" | grep -qi "error\|ERR"; then
    echo "   ✗ Command returned error: $result"
    exit 1
fi

# Verify result contains patch data (Debug format starts with '[')
if echo "$result" | grep -q "^\["; then
    echo "   ✓ Diff from empty to current state returns patch data"
else
    echo "   ✗ Result doesn't look like patch data: $result"
    exit 1
fi

echo "Test 2: Diff with same state (should be empty)..."
redis-cli -h "$HOST" del diff_test2 > /dev/null
redis-cli -h "$HOST" am.new diff_test2 > /dev/null
redis-cli -h "$HOST" am.puttext diff_test2 status "draft" > /dev/null

# Get diff comparing empty to empty (should be empty)
result=$(redis-cli -h "$HOST" am.getdiff diff_test2 BEFORE AFTER 2>&1)

# Verify result is not an error
if echo "$result" | grep -qi "error\|ERR"; then
    echo "   ✗ Command returned error: $result"
    exit 1
fi

echo "   ✓ Diff command executes successfully"

echo "Test 3: Diff with nested structures..."
redis-cli -h "$HOST" del diff_test3 > /dev/null
redis-cli -h "$HOST" am.new diff_test3 > /dev/null
redis-cli -h "$HOST" am.puttext diff_test3 user.name "Alice" > /dev/null
redis-cli -h "$HOST" am.putint diff_test3 user.age 25 > /dev/null

# Get diff
result=$(redis-cli -h "$HOST" am.getdiff diff_test3 BEFORE AFTER 2>&1)

# Verify result is not an error
if echo "$result" | grep -qi "error\|ERR"; then
    echo "   ✗ Command returned error: $result"
    exit 1
fi

echo "   ✓ Diff works with nested structures"

echo "Test 4: Diff with lists..."
redis-cli -h "$HOST" del diff_test4 > /dev/null
redis-cli -h "$HOST" am.new diff_test4 > /dev/null
redis-cli -h "$HOST" am.createlist diff_test4 items > /dev/null
redis-cli -h "$HOST" am.appendtext diff_test4 items "item1" > /dev/null
redis-cli -h "$HOST" am.appendtext diff_test4 items "item2" > /dev/null

# Get diff
result=$(redis-cli -h "$HOST" am.getdiff diff_test4 BEFORE AFTER 2>&1)

# Verify result is not an error
if echo "$result" | grep -qi "error\|ERR"; then
    echo "   ✗ Command returned error: $result"
    exit 1
fi

echo "   ✓ Diff works with list operations"

echo "Test 5: Diff command syntax validation..."
redis-cli -h "$HOST" del diff_test5 > /dev/null
redis-cli -h "$HOST" am.new diff_test5 > /dev/null

# Verify command runs without error with empty hash lists
result=$(redis-cli -h "$HOST" am.getdiff diff_test5 BEFORE AFTER 2>&1)
if echo "$result" | grep -qi "error\|ERR"; then
    echo "   ✗ Valid command syntax returned error: $result"
    exit 1
fi

echo "   ✓ Command accepts valid syntax"

echo "Test 6: Error handling - missing BEFORE keyword..."
redis-cli -h "$HOST" del diff_test6 > /dev/null
redis-cli -h "$HOST" am.new diff_test6 > /dev/null

# Try diff without BEFORE keyword
result=$(redis-cli -h "$HOST" am.getdiff diff_test6 AFTER 2>&1 || true)

# Accept either "missing BEFORE" or "wrong number of arguments" errors
if echo "$result" | grep -qi "BEFORE\|wrong.*arguments"; then
    echo "   ✓ Missing BEFORE keyword returns appropriate error"
else
    echo "   ✗ Expected error about missing BEFORE keyword or wrong arguments, got: $result"
    exit 1
fi

echo "Test 7: Error handling - missing AFTER keyword..."
# Try diff without AFTER keyword
result=$(redis-cli -h "$HOST" am.getdiff diff_test6 BEFORE 2>&1 || true)

# Accept either "missing AFTER" or "wrong number of arguments" errors
if echo "$result" | grep -qi "AFTER\|wrong.*arguments"; then
    echo "   ✓ Missing AFTER keyword returns appropriate error"
else
    echo "   ✗ Expected error about missing AFTER keyword or wrong arguments, got: $result"
    exit 1
fi

echo "Test 8: Error handling - wrong keyword order..."
# Try diff with AFTER before BEFORE - this should error
result=$(redis-cli -h "$HOST" am.getdiff diff_test6 AFTER BEFORE 2>&1 || true)

if echo "$result" | grep -qi "BEFORE.*AFTER\|missing.*BEFORE\|wrong.*arguments"; then
    echo "   ✓ Wrong keyword order returns appropriate error"
else
    echo "   ✗ Expected error about keyword order, got: $result"
    exit 1
fi


echo ""
echo "✅ All diff tests passed!"
