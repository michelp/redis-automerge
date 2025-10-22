#!/usr/bin/env bash
# Test timestamp operations (AM.PUTTIMESTAMP and AM.GETTIMESTAMP)

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Timestamp Operations"

echo "Test 1: Timestamp get/set operations..."
redis-cli -h "$HOST" del ts_test1 > /dev/null
redis-cli -h "$HOST" am.new ts_test1 > /dev/null
# Unix timestamp for 2024-01-01T00:00:00Z in milliseconds
redis-cli -h "$HOST" am.puttimestamp ts_test1 created_at 1704067200000 > /dev/null
val=$(redis-cli -h "$HOST" am.gettimestamp ts_test1 created_at)
assert_equals "$val" "1704067200000"
echo "   ✓ Timestamp get/set works"

echo "Test 2: Timestamp persistence..."
redis-cli -h "$HOST" --raw am.save ts_test1 > /tmp/ts-saved.bin
truncate -s -1 /tmp/ts-saved.bin
redis-cli -h "$HOST" del ts_test1 > /dev/null
redis-cli -h "$HOST" --raw -x am.load ts_test1 < /tmp/ts-saved.bin > /dev/null
val=$(redis-cli -h "$HOST" am.gettimestamp ts_test1 created_at)
assert_equals "$val" "1704067200000"
echo "   ✓ Timestamp persistence works"
rm -f /tmp/ts-saved.bin

echo "Test 3: Timestamp in nested paths..."
redis-cli -h "$HOST" del ts_test2 > /dev/null
redis-cli -h "$HOST" am.new ts_test2 > /dev/null
redis-cli -h "$HOST" am.puttimestamp ts_test2 event.created_at 1704067200000 > /dev/null
val=$(redis-cli -h "$HOST" am.gettimestamp ts_test2 event.created_at)
assert_equals "$val" "1704067200000"
echo "   ✓ Nested timestamp paths work"

echo "Test 4: Timestamp JSON export as ISO 8601..."
redis-cli -h "$HOST" del ts_test3 > /dev/null
redis-cli -h "$HOST" am.new ts_test3 > /dev/null
redis-cli -h "$HOST" am.puttimestamp ts_test3 created_at 1704067200000 > /dev/null
redis-cli -h "$HOST" am.puttext ts_test3 name "Event" > /dev/null
json=$(redis-cli -h "$HOST" --raw am.tojson ts_test3)
# Timestamp should be exported as ISO 8601 datetime string
if echo "$json" | grep -q "2024-01-01T00:00:00+00:00"; then
    echo "   ✓ Timestamps export as ISO 8601 UTC strings in JSON"
else
    echo "   ✗ Timestamp JSON export failed: $json"
    exit 1
fi

echo "Test 5: Timestamp notification..."
redis-cli -h "$HOST" del notif_ts > /dev/null
redis-cli -h "$HOST" am.new notif_ts > /dev/null
test_notification "notif_ts" "am.puttimestamp" "redis-cli -h $HOST am.puttimestamp notif_ts event_time 1704067200000"
echo "   ✓ AM.PUTTIMESTAMP emits keyspace notification"

echo "Test 6: Timestamp change publishing..."
redis-cli -h "$HOST" del change_pub_ts > /dev/null
redis-cli -h "$HOST" am.new change_pub_ts > /dev/null
test_change_publication "change_pub_ts" "redis-cli -h $HOST am.puttimestamp change_pub_ts created_at 1704067200000"
echo "   ✓ AM.PUTTIMESTAMP publishes changes to changes:key channel"

echo "Test 7: Mixed types with timestamps..."
redis-cli -h "$HOST" del ts_test4 > /dev/null
redis-cli -h "$HOST" am.new ts_test4 > /dev/null
redis-cli -h "$HOST" am.puttext ts_test4 name "Alice" > /dev/null
redis-cli -h "$HOST" am.putint ts_test4 age 30 > /dev/null
redis-cli -h "$HOST" am.puttimestamp ts_test4 joined_at 1704067200000 > /dev/null
val=$(redis-cli -h "$HOST" am.gettimestamp ts_test4 joined_at)
assert_equals "$val" "1704067200000"
echo "   ✓ Timestamps work alongside other types"

echo ""
echo "✅ All timestamp operation tests passed!"
