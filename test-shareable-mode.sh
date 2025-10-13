#!/bin/bash

# Test script for ShareableMode implementation
# This tests the Redis backend functionality that ShareableMode relies on

set -e

echo "========================================="
echo "Testing ShareableMode Backend Operations"
echo "========================================="
echo ""

# Color codes for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

test_passed=0
test_failed=0

# Helper function to run tests
run_test() {
    local test_name="$1"
    local command="$2"
    local expected="$3"

    echo -n "Testing: $test_name... "
    result=$(eval "$command")

    if [[ "$result" == *"$expected"* ]]; then
        echo -e "${GREEN}✓ PASSED${NC}"
        ((test_passed++))
    else
        echo -e "${RED}✗ FAILED${NC}"
        echo "  Expected: $expected"
        echo "  Got: $result"
        ((test_failed++))
    fi
}

# Clean up any existing test rooms
echo "Cleaning up test rooms..."
redis-cli KEYS "am:room:test-*" | xargs -r redis-cli DEL > /dev/null 2>&1 || true
echo ""

# Test 1: Check keyspace notifications are enabled
echo "Test 1: Keyspace Notifications"
echo "==============================="
run_test "Keyspace notifications enabled" \
    "redis-cli CONFIG GET notify-keyspace-events | tail -1" \
    "AKE"
echo ""

# Test 2: Create a room
echo "Test 2: Room Creation"
echo "====================="
run_test "Create room with AM.NEW" \
    "redis-cli AM.NEW 'am:room:test-create'" \
    "OK"

run_test "Initialize room text field" \
    "redis-cli AM.PUTTEXT 'am:room:test-create' text ''" \
    "OK"

run_test "Verify room exists" \
    "redis-cli EXISTS 'am:room:test-create'" \
    "1"
echo ""

# Test 3: Room text operations
echo "Test 3: Room Text Operations"
echo "============================"
run_test "Set room text" \
    "redis-cli AM.PUTTEXT 'am:room:test-ops' text 'Hello World'" \
    "OK"

run_test "Get room text" \
    "redis-cli AM.GETTEXT 'am:room:test-ops' text" \
    "Hello World"

run_test "Splice text operation" \
    "redis-cli AM.SPLICETEXT 'am:room:test-ops' text 6 5 'Redis'" \
    "OK"

run_test "Verify spliced text" \
    "redis-cli AM.GETTEXT 'am:room:test-ops' text" \
    "Hello Redis"
echo ""

# Test 4: Room list discovery
echo "Test 4: Room List Discovery"
echo "============================"
# Create multiple rooms
redis-cli AM.NEW 'am:room:test-room-1' > /dev/null
redis-cli AM.NEW 'am:room:test-room-2' > /dev/null
redis-cli AM.NEW 'am:room:test-room-3' > /dev/null

room_count=$(redis-cli KEYS 'am:room:test-*' | wc -l)
echo -n "Testing: Find all test rooms... "
if [ "$room_count" -ge 5 ]; then
    echo -e "${GREEN}✓ PASSED${NC} (found $room_count rooms)"
    ((test_passed++))
else
    echo -e "${RED}✗ FAILED${NC} (found $room_count rooms, expected >= 5)"
    ((test_failed++))
fi
echo ""

# Test 5: Active user tracking
echo "Test 5: Active User Tracking"
echo "============================"
run_test "Check PUBSUB NUMSUB command" \
    "redis-cli PUBSUB NUMSUB 'changes:am:room:test-room-1' | tail -1" \
    "0"
echo ""

# Test 6: Room persistence
echo "Test 6: Room Persistence"
echo "========================"
redis-cli AM.PUTTEXT 'am:room:test-persist' text 'Persistent data' > /dev/null
run_test "Room data persists after set" \
    "redis-cli AM.GETTEXT 'am:room:test-persist' text" \
    "Persistent data"
echo ""

# Test 7: Room name validation (via pattern matching)
echo "Test 7: Room Name Patterns"
echo "=========================="
# These should all be valid room names
for name in "test-123" "test_abc" "TestRoom" "room123"; do
    redis-cli AM.NEW "am:room:$name" > /dev/null 2>&1
    exists=$(redis-cli EXISTS "am:room:$name")
    echo -n "Testing: Valid room name '$name'... "
    if [ "$exists" == "1" ]; then
        echo -e "${GREEN}✓ PASSED${NC}"
        ((test_passed++))
    else
        echo -e "${RED}✗ FAILED${NC}"
        ((test_failed++))
    fi
done
echo ""

# Clean up all test rooms
echo "Cleaning up test rooms..."
redis-cli KEYS "am:room:test-*" | xargs -r redis-cli DEL > /dev/null 2>&1 || true
redis-cli KEYS "am:room:room123" | xargs -r redis-cli DEL > /dev/null 2>&1 || true
redis-cli KEYS "am:room:TestRoom" | xargs -r redis-cli DEL > /dev/null 2>&1 || true
echo ""

# Summary
echo "========================================="
echo "Test Summary"
echo "========================================="
echo -e "${GREEN}Passed: $test_passed${NC}"
echo -e "${RED}Failed: $test_failed${NC}"
echo ""

if [ $test_failed -eq 0 ]; then
    echo -e "${GREEN}All tests passed! ✓${NC}"
    echo ""
    echo "ShareableMode backend is ready for use."
    echo "You can now test the UI in your browser:"
    echo "  1. Open demo/editor.html"
    echo "  2. Click 'Shareable Link Mode' tab"
    echo "  3. Create a room and test collaboration"
    exit 0
else
    echo -e "${RED}Some tests failed! ✗${NC}"
    exit 1
fi
