#!/usr/bin/env bash
# Test automatic change publication to changes:{key} channels

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Change Publishing (Pub/Sub)"

echo "Test 1: AM.PUTTEXT change publishing..."
test_change_publication "change_pub_test1" "redis-cli -h $HOST am.puttext change_pub_test1 field 'test value'"
echo "   ✓ AM.PUTTEXT publishes changes to changes:key channel"

echo "Test 2: Change synchronization between documents..."
redis-cli -h "$HOST" del sync_test_doc1 > /dev/null
redis-cli -h "$HOST" am.new sync_test_doc1 > /dev/null
test_change_publication "sync_test_doc1" "redis-cli -h $HOST am.puttext sync_test_doc1 name Alice"
echo "   ✓ Changes are published and can be subscribed to"

echo "Test 3: Multiple changes publish correctly..."
redis-cli -h "$HOST" del multi_change_test > /dev/null
redis-cli -h "$HOST" am.new multi_change_test > /dev/null
timeout 3 redis-cli -h "$HOST" SUBSCRIBE "changes:multi_change_test" > /tmp/multi_test.txt 2>&1 &
sub_pid=$!
sleep 0.3
redis-cli -h "$HOST" am.puttext multi_change_test name "Bob" > /dev/null 2>&1
sleep 0.2
redis-cli -h "$HOST" am.putint multi_change_test age 25 > /dev/null 2>&1
sleep 0.2
redis-cli -h "$HOST" am.putbool multi_change_test active true > /dev/null 2>&1
sleep 0.3
kill $sub_pid 2>/dev/null || true
wait $sub_pid 2>/dev/null || true
change_count=$(grep -c "changes:multi_change_test" /tmp/multi_test.txt || echo 0)
if [ "$change_count" -ge 3 ]; then
    echo "   ✓ Multiple changes publish correctly (found $change_count publications)"
else
    echo "   ✗ Expected 3+ publications, found $change_count"
    exit 1
fi
rm -f /tmp/multi_test.txt

echo "Test 4: AM.PUTDOUBLE change publishing..."
test_change_publication "change_pub_double" "redis-cli -h $HOST am.putdouble change_pub_double pi 3.14159"
echo "   ✓ AM.PUTDOUBLE publishes changes"

echo "Test 5: AM.PUTCOUNTER change publishing..."
test_change_publication "change_pub_counter" "redis-cli -h $HOST am.putcounter change_pub_counter views 100"
echo "   ✓ AM.PUTCOUNTER publishes changes"

echo "Test 6: AM.INCCOUNTER change publishing..."
redis-cli -h "$HOST" del change_pub_inccounter > /dev/null
redis-cli -h "$HOST" am.new change_pub_inccounter > /dev/null
redis-cli -h "$HOST" am.putcounter change_pub_inccounter likes 10 > /dev/null
test_change_publication "change_pub_inccounter" "redis-cli -h $HOST am.inccounter change_pub_inccounter likes 5"
echo "   ✓ AM.INCCOUNTER publishes changes"

echo "Test 7: AM.PUTDIFF change publishing..."
redis-cli -h "$HOST" del change_pub_diff > /dev/null
redis-cli -h "$HOST" am.new change_pub_diff > /dev/null
redis-cli -h "$HOST" am.puttext change_pub_diff content "Hello World" > /dev/null
test_change_publication "change_pub_diff" "redis-cli -h $HOST am.putdiff change_pub_diff content '--- a/content
+++ b/content
@@ -1 +1 @@
-Hello World
+Hello Redis
'"
echo "   ✓ AM.PUTDIFF publishes changes"

echo "Test 8: AM.CREATELIST change publishing..."
test_change_publication "change_pub_list" "redis-cli -h $HOST am.createlist change_pub_list items"
echo "   ✓ AM.CREATELIST publishes changes"

echo "Test 9: AM.APPENDTEXT change publishing..."
redis-cli -h "$HOST" del change_pub_appendtext > /dev/null
redis-cli -h "$HOST" am.new change_pub_appendtext > /dev/null
redis-cli -h "$HOST" am.createlist change_pub_appendtext items > /dev/null
test_change_publication "change_pub_appendtext" "redis-cli -h $HOST am.appendtext change_pub_appendtext items 'text value'"
echo "   ✓ AM.APPENDTEXT publishes changes"

echo "Test 10: AM.APPENDINT change publishing..."
redis-cli -h "$HOST" del change_pub_appendint > /dev/null
redis-cli -h "$HOST" am.new change_pub_appendint > /dev/null
redis-cli -h "$HOST" am.createlist change_pub_appendint numbers > /dev/null
test_change_publication "change_pub_appendint" "redis-cli -h $HOST am.appendint change_pub_appendint numbers 42"
echo "   ✓ AM.APPENDINT publishes changes"

echo "Test 11: AM.APPENDDOUBLE change publishing..."
redis-cli -h "$HOST" del change_pub_appenddouble > /dev/null
redis-cli -h "$HOST" am.new change_pub_appenddouble > /dev/null
redis-cli -h "$HOST" am.createlist change_pub_appenddouble values > /dev/null
test_change_publication "change_pub_appenddouble" "redis-cli -h $HOST am.appenddouble change_pub_appenddouble values 2.71828"
echo "   ✓ AM.APPENDDOUBLE publishes changes"

echo "Test 12: AM.APPENDBOOL change publishing..."
redis-cli -h "$HOST" del change_pub_appendbool > /dev/null
redis-cli -h "$HOST" am.new change_pub_appendbool > /dev/null
redis-cli -h "$HOST" am.createlist change_pub_appendbool flags > /dev/null
test_change_publication "change_pub_appendbool" "redis-cli -h $HOST am.appendbool change_pub_appendbool flags true"
echo "   ✓ AM.APPENDBOOL publishes changes"

echo "Test 13: AM.SPLICETEXT change publishing..."
redis-cli -h "$HOST" del change_pub_splice > /dev/null
redis-cli -h "$HOST" am.new change_pub_splice > /dev/null
redis-cli -h "$HOST" am.puttext change_pub_splice content "Hello World" > /dev/null
test_change_publication "change_pub_splice" "redis-cli -h $HOST am.splicetext change_pub_splice content 6 5 Rust"
echo "   ✓ AM.SPLICETEXT publishes changes"

echo "Test 14: AM.PUTTIMESTAMP change publishing..."
redis-cli -h "$HOST" del change_pub_ts > /dev/null
redis-cli -h "$HOST" am.new change_pub_ts > /dev/null
test_change_publication "change_pub_ts" "redis-cli -h $HOST am.puttimestamp change_pub_ts created_at 1704067200000"
echo "   ✓ AM.PUTTIMESTAMP publishes changes"

echo "Test 15: All list operations publish changes..."
redis-cli -h "$HOST" del change_pub_all_list > /dev/null
redis-cli -h "$HOST" am.new change_pub_all_list > /dev/null
timeout 4 redis-cli -h "$HOST" SUBSCRIBE "changes:change_pub_all_list" > /tmp/changes_all_list.txt 2>&1 &
sub_pid=$!
sleep 0.3
redis-cli -h "$HOST" am.createlist change_pub_all_list items > /dev/null 2>&1
sleep 0.2
redis-cli -h "$HOST" am.appendtext change_pub_all_list items "text" > /dev/null 2>&1
sleep 0.2
redis-cli -h "$HOST" am.appendint change_pub_all_list items 100 > /dev/null 2>&1
sleep 0.2
redis-cli -h "$HOST" am.appenddouble change_pub_all_list items 1.5 > /dev/null 2>&1
sleep 0.2
redis-cli -h "$HOST" am.appendbool change_pub_all_list items true > /dev/null 2>&1
sleep 0.3
kill $sub_pid 2>/dev/null || true
wait $sub_pid 2>/dev/null || true
change_count=$(grep -c "changes:change_pub_all_list" /tmp/changes_all_list.txt || echo 0)
if [ "$change_count" -ge 5 ]; then
    echo "   ✓ All list operations publish changes (found $change_count publications)"
else
    echo "   ✗ Expected 5+ publications, found $change_count"
    exit 1
fi
rm -f /tmp/changes_all_list.txt

echo ""
echo "✅ All change publishing tests passed!"
