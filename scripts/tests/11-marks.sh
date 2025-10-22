#!/usr/bin/env bash
# Test marks operations: AM.MARKCREATE, AM.MARKCLEAR, AM.MARKS

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Marks Operations"

# Test AM.MARKCREATE with string value
echo "Test 1: AM.MARKCREATE with string value..."
redis-cli -h "$HOST" del marks_test1 > /dev/null
redis-cli -h "$HOST" am.new marks_test1 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test1 content "Hello World" > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_test1 content bold true 0 5)
assert_equals "$result" "OK"
echo "   ✓ AM.MARKCREATE creates mark with string value"

# Test AM.MARKS retrieves marks
echo "Test 2: AM.MARKS retrieves marks..."
marks=$(redis-cli -h "$HOST" am.marks marks_test1 content)
# Should return array with [name, value, start, end]
if [[ "$marks" == *"bold"* ]]; then
    echo "   ✓ AM.MARKS returns mark information"
else
    echo "   ✗ AM.MARKS did not return expected mark"
    exit 1
fi

# Test AM.MARKCREATE with boolean value
echo "Test 3: AM.MARKCREATE with boolean value..."
redis-cli -h "$HOST" del marks_test2 > /dev/null
redis-cli -h "$HOST" am.new marks_test2 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test2 text "Testing booleans" > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_test2 text italic true 0 7)
assert_equals "$result" "OK"
echo "   ✓ AM.MARKCREATE handles boolean values"

# Test AM.MARKCREATE with integer value
echo "Test 4: AM.MARKCREATE with integer value..."
redis-cli -h "$HOST" del marks_test3 > /dev/null
redis-cli -h "$HOST" am.new marks_test3 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test3 data "Sample text" > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_test3 data fontSize 14 0 6)
assert_equals "$result" "OK"
echo "   ✓ AM.MARKCREATE handles integer values"

# Test AM.MARKCREATE with float value
echo "Test 5: AM.MARKCREATE with float value..."
redis-cli -h "$HOST" del marks_test4 > /dev/null
redis-cli -h "$HOST" am.new marks_test4 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test4 paragraph "Floating point test" > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_test4 paragraph opacity 0.75 0 8)
assert_equals "$result" "OK"
echo "   ✓ AM.MARKCREATE handles float values"

# Test multiple marks on same text
echo "Test 6: Multiple marks on same text..."
redis-cli -h "$HOST" del marks_test5 > /dev/null
redis-cli -h "$HOST" am.new marks_test5 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test5 rich "Rich text here" > /dev/null
redis-cli -h "$HOST" am.markcreate marks_test5 rich bold true 0 4 > /dev/null
redis-cli -h "$HOST" am.markcreate marks_test5 rich italic true 5 9 > /dev/null
redis-cli -h "$HOST" am.markcreate marks_test5 rich underline true 10 14 > /dev/null
marks=$(redis-cli -h "$HOST" am.marks marks_test5 rich)
if [[ "$marks" == *"bold"* ]] && [[ "$marks" == *"italic"* ]] && [[ "$marks" == *"underline"* ]]; then
    echo "   ✓ Multiple marks can be applied to same text"
else
    echo "   ✗ Multiple marks test failed"
    exit 1
fi

# Test AM.MARKCLEAR
echo "Test 7: AM.MARKCLEAR removes marks..."
redis-cli -h "$HOST" del marks_test6 > /dev/null
redis-cli -h "$HOST" am.new marks_test6 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test6 styled "Styled content" > /dev/null
redis-cli -h "$HOST" am.markcreate marks_test6 styled emphasis true 0 6 > /dev/null
# Verify mark exists
marks_before=$(redis-cli -h "$HOST" am.marks marks_test6 styled)
if [[ "$marks_before" != *"emphasis"* ]]; then
    echo "   ✗ Mark was not created before clear"
    exit 1
fi
# Clear the mark
result=$(redis-cli -h "$HOST" am.markclear marks_test6 styled emphasis 0 6)
assert_equals "$result" "OK"
# Verify mark is gone
marks_after=$(redis-cli -h "$HOST" am.marks marks_test6 styled)
if [[ "$marks_after" == *"emphasis"* ]]; then
    echo "   ✗ Mark was not cleared"
    exit 1
fi
echo "   ✓ AM.MARKCLEAR removes marks"

# Test marks with expand parameter (None)
echo "Test 8: AM.MARKCREATE with expand=none..."
redis-cli -h "$HOST" del marks_test7 > /dev/null
redis-cli -h "$HOST" am.new marks_test7 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test7 text "Expandable text" > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_test7 text highlight yellow 0 10 none)
assert_equals "$result" "OK"
echo "   ✓ AM.MARKCREATE accepts expand parameter (none)"

# Test marks with expand parameter (before)
echo "Test 9: AM.MARKCREATE with expand=before..."
redis-cli -h "$HOST" del marks_test8 > /dev/null
redis-cli -h "$HOST" am.new marks_test8 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test8 text "More text" > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_test8 text code gray 0 4 before)
assert_equals "$result" "OK"
echo "   ✓ AM.MARKCREATE accepts expand parameter (before)"

# Test marks with expand parameter (after)
echo "Test 10: AM.MARKCREATE with expand=after..."
redis-cli -h "$HOST" del marks_test9 > /dev/null
redis-cli -h "$HOST" am.new marks_test9 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test9 text "After test" > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_test9 text link blue 0 5 after)
assert_equals "$result" "OK"
echo "   ✓ AM.MARKCREATE accepts expand parameter (after)"

# Test marks with expand parameter (both)
echo "Test 11: AM.MARKCREATE with expand=both..."
redis-cli -h "$HOST" del marks_test10 > /dev/null
redis-cli -h "$HOST" am.new marks_test10 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test10 text "Both sides" > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_test10 text annotation green 0 4 both)
assert_equals "$result" "OK"
echo "   ✓ AM.MARKCREATE accepts expand parameter (both)"

# Test marks on nested paths
echo "Test 12: Marks on nested paths..."
redis-cli -h "$HOST" del marks_test11 > /dev/null
redis-cli -h "$HOST" am.new marks_test11 > /dev/null
redis-cli -h "$HOST" am.puttext marks_test11 doc.title "Nested Document Title" > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_test11 doc.title heading true 0 6)
assert_equals "$result" "OK"
marks=$(redis-cli -h "$HOST" am.marks marks_test11 doc.title)
if [[ "$marks" == *"heading"* ]]; then
    echo "   ✓ Marks work on nested paths"
else
    echo "   ✗ Marks on nested paths failed"
    exit 1
fi

# Test marks persistence
echo "Test 13: Marks persist through save/load..."
redis-cli -h "$HOST" del marks_persist > /dev/null
redis-cli -h "$HOST" am.new marks_persist > /dev/null
redis-cli -h "$HOST" am.puttext marks_persist content "Persistent marks" > /dev/null
redis-cli -h "$HOST" am.markcreate marks_persist content important true 0 10 > /dev/null
# Save document
redis-cli -h "$HOST" --raw am.save marks_persist > /tmp/marks_persist.bin
truncate -s -1 /tmp/marks_persist.bin
# Get marks before reload
marks_before=$(redis-cli -h "$HOST" am.marks marks_persist content)
# Delete and reload
redis-cli -h "$HOST" del marks_persist > /dev/null
redis-cli -h "$HOST" --raw -x am.load marks_persist < /tmp/marks_persist.bin > /dev/null
# Get marks after reload
marks_after=$(redis-cli -h "$HOST" am.marks marks_persist content)
if [[ "$marks_after" == *"important"* ]]; then
    echo "   ✓ Marks persist through save/load"
else
    echo "   ✗ Marks not persisted correctly"
    exit 1
fi
rm -f /tmp/marks_persist.bin

# Test AM.MARKCREATE on Text object (after splicetext)
echo "Test 14: Marks on Text objects..."
redis-cli -h "$HOST" del marks_text_obj > /dev/null
redis-cli -h "$HOST" am.new marks_text_obj > /dev/null
redis-cli -h "$HOST" am.puttext marks_text_obj story "Original story" > /dev/null
# Convert to Text object via splice
redis-cli -h "$HOST" am.splicetext marks_text_obj story 0 0 "" > /dev/null
# Now add marks
result=$(redis-cli -h "$HOST" am.markcreate marks_text_obj story chapter true 0 8)
assert_equals "$result" "OK"
marks=$(redis-cli -h "$HOST" am.marks marks_text_obj story)
if [[ "$marks" == *"chapter"* ]]; then
    echo "   ✓ Marks work on Text objects"
else
    echo "   ✗ Marks on Text objects failed"
    exit 1
fi

# Test error: marks on non-text field
echo "Test 15: Error when marking non-text field..."
redis-cli -h "$HOST" del marks_error1 > /dev/null
redis-cli -h "$HOST" am.new marks_error1 > /dev/null
redis-cli -h "$HOST" am.putint marks_error1 number 42 > /dev/null
result=$(redis-cli -h "$HOST" am.markcreate marks_error1 number bold true 0 2 2>&1 || true)
# Check if command failed (result should not be "OK" and should have error-like content)
if [[ "$result" != "OK" ]] && [[ -n "$result" ]]; then
    echo "   ✓ Error when marking non-text field"
else
    echo "   ✗ Should have errored on non-text field (got: $result)"
    exit 1
fi

# Test marks with overlapping ranges
echo "Test 16: Overlapping mark ranges..."
redis-cli -h "$HOST" del marks_overlap > /dev/null
redis-cli -h "$HOST" am.new marks_overlap > /dev/null
redis-cli -h "$HOST" am.puttext marks_overlap text "Overlapping marks test" > /dev/null
redis-cli -h "$HOST" am.markcreate marks_overlap text bold true 0 11 > /dev/null
redis-cli -h "$HOST" am.markcreate marks_overlap text italic true 6 17 > /dev/null
marks=$(redis-cli -h "$HOST" am.marks marks_overlap text)
if [[ "$marks" == *"bold"* ]] && [[ "$marks" == *"italic"* ]]; then
    echo "   ✓ Overlapping mark ranges supported"
else
    echo "   ✗ Overlapping marks test failed"
    exit 1
fi

# Test AM.MARKCLEAR with expand parameter
echo "Test 17: AM.MARKCLEAR with expand parameter..."
redis-cli -h "$HOST" del marks_clear_expand > /dev/null
redis-cli -h "$HOST" am.new marks_clear_expand > /dev/null
redis-cli -h "$HOST" am.puttext marks_clear_expand text "Clear with expand" > /dev/null
redis-cli -h "$HOST" am.markcreate marks_clear_expand text temp red 0 5 both > /dev/null
result=$(redis-cli -h "$HOST" am.markclear marks_clear_expand text temp 0 5 both)
assert_equals "$result" "OK"
echo "   ✓ AM.MARKCLEAR accepts expand parameter"

echo ""
echo "✅ All marks tests passed!"
