#!/usr/bin/env bash
# Test advanced text operations: AM.PUTDIFF and AM.SPLICETEXT

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Text Operations (PUTDIFF & SPLICETEXT)"

echo "Test 1: AM.PUTDIFF with simple replacement..."
redis-cli -h "$HOST" del diff_test1 > /dev/null
redis-cli -h "$HOST" am.new diff_test1 > /dev/null
redis-cli -h "$HOST" am.puttext diff_test1 content "Hello World" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext diff_test1 content)
assert_equals "$val" "Hello World"

# Apply a diff that changes "World" to "Rust"
redis-cli -h "$HOST" am.putdiff diff_test1 content "--- a/content
+++ b/content
@@ -1 +1 @@
-Hello World
+Hello Rust
" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext diff_test1 content)
assert_equals "$val" "Hello Rust"
echo "   ✓ AM.PUTDIFF simple replacement works"

echo "Test 2: AM.PUTDIFF with line insertion..."
redis-cli -h "$HOST" del diff_test2 > /dev/null
redis-cli -h "$HOST" am.new diff_test2 > /dev/null
printf "Line 1\nLine 3\n" | redis-cli -h "$HOST" -x am.puttext diff_test2 doc > /dev/null

# Apply a diff that inserts "Line 2"
printf -- "--- a/doc\n+++ b/doc\n@@ -1,2 +1,3 @@\n Line 1\n+Line 2\n Line 3\n" | redis-cli -h "$HOST" -x am.putdiff diff_test2 doc > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext diff_test2 doc)
expected=$(printf "Line 1\nLine 2\nLine 3\n")
assert_equals "$val" "$expected"
echo "   ✓ AM.PUTDIFF line insertion works"

echo "Test 3: AM.PUTDIFF with line deletion..."
redis-cli -h "$HOST" del diff_test3 > /dev/null
redis-cli -h "$HOST" am.new diff_test3 > /dev/null
printf "Line 1\nLine 2\nLine 3\n" | redis-cli -h "$HOST" -x am.puttext diff_test3 doc > /dev/null

# Apply a diff that removes Line 2
printf -- "--- a/doc\n+++ b/doc\n@@ -1,3 +1,2 @@\n Line 1\n-Line 2\n Line 3\n" | redis-cli -h "$HOST" -x am.putdiff diff_test3 doc > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext diff_test3 doc)
expected=$(printf "Line 1\nLine 3\n")
assert_equals "$val" "$expected"
echo "   ✓ AM.PUTDIFF line deletion works"

# Test AM.SPLICETEXT command
echo "Test 4: AM.SPLICETEXT with simple replacement..."
redis-cli -h "$HOST" del splice_test1 > /dev/null
redis-cli -h "$HOST" am.new splice_test1 > /dev/null
redis-cli -h "$HOST" am.puttext splice_test1 greeting "Hello World" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext splice_test1 greeting)
assert_equals "$val" "Hello World"

# Replace "World" with "Rust" - delete 5 chars at position 6, insert "Rust"
redis-cli -h "$HOST" am.splicetext splice_test1 greeting 6 5 "Rust" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext splice_test1 greeting)
assert_equals "$val" "Hello Rust"
echo "   ✓ AM.SPLICETEXT simple replacement works"

echo "Test 5: AM.SPLICETEXT with insertion..."
redis-cli -h "$HOST" del splice_test2 > /dev/null
redis-cli -h "$HOST" am.new splice_test2 > /dev/null
redis-cli -h "$HOST" am.puttext splice_test2 text "HelloWorld" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext splice_test2 text)
assert_equals "$val" "HelloWorld"

# Insert a space at position 5 - delete 0, insert " "
redis-cli -h "$HOST" am.splicetext splice_test2 text 5 0 " " > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext splice_test2 text)
assert_equals "$val" "Hello World"
echo "   ✓ AM.SPLICETEXT insertion works"

echo "Test 6: AM.SPLICETEXT with deletion..."
redis-cli -h "$HOST" del splice_test3 > /dev/null
redis-cli -h "$HOST" am.new splice_test3 > /dev/null
redis-cli -h "$HOST" am.puttext splice_test3 text "Hello  World" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext splice_test3 text)
assert_equals "$val" "Hello  World"

# Delete extra space at position 5 - delete 1, insert nothing
redis-cli -h "$HOST" am.splicetext splice_test3 text 5 1 "" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext splice_test3 text)
assert_equals "$val" "Hello World"
echo "   ✓ AM.SPLICETEXT deletion works"

echo "Test 7: AM.SPLICETEXT at beginning..."
redis-cli -h "$HOST" del splice_test4 > /dev/null
redis-cli -h "$HOST" am.new splice_test4 > /dev/null
redis-cli -h "$HOST" am.puttext splice_test4 text "World" > /dev/null

# Insert at beginning
redis-cli -h "$HOST" am.splicetext splice_test4 text 0 0 "Hello " > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext splice_test4 text)
assert_equals "$val" "Hello World"
echo "   ✓ AM.SPLICETEXT at beginning works"

echo "Test 8: AM.SPLICETEXT at end..."
redis-cli -h "$HOST" del splice_test5 > /dev/null
redis-cli -h "$HOST" am.new splice_test5 > /dev/null
redis-cli -h "$HOST" am.puttext splice_test5 text "Hello" > /dev/null

# Insert at end
redis-cli -h "$HOST" am.splicetext splice_test5 text 5 0 " World" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext splice_test5 text)
assert_equals "$val" "Hello World"
echo "   ✓ AM.SPLICETEXT at end works"

echo "Test 9: AM.SPLICETEXT with nested path..."
redis-cli -h "$HOST" del splice_test6 > /dev/null
redis-cli -h "$HOST" am.new splice_test6 > /dev/null
redis-cli -h "$HOST" am.puttext splice_test6 user.greeting "Hello World" > /dev/null

# Splice nested path
redis-cli -h "$HOST" am.splicetext splice_test6 user.greeting 6 5 "Rust" > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext splice_test6 user.greeting)
assert_equals "$val" "Hello Rust"
echo "   ✓ AM.SPLICETEXT with nested paths works"

echo "Test 10: AM.SPLICETEXT persistence..."
redis-cli -h "$HOST" del splice_test7 > /dev/null
redis-cli -h "$HOST" am.new splice_test7 > /dev/null
redis-cli -h "$HOST" am.puttext splice_test7 doc "Hello World" > /dev/null
redis-cli -h "$HOST" am.splicetext splice_test7 doc 6 5 "Rust" > /dev/null

# Save and reload
redis-cli -h "$HOST" --raw am.save splice_test7 > /tmp/splice-saved.bin
truncate -s -1 /tmp/splice-saved.bin
redis-cli -h "$HOST" del splice_test7 > /dev/null
redis-cli -h "$HOST" --raw -x am.load splice_test7 < /tmp/splice-saved.bin > /dev/null

val=$(redis-cli -h "$HOST" --raw am.gettext splice_test7 doc)
assert_equals "$val" "Hello Rust"
echo "   ✓ AM.SPLICETEXT persistence works"

rm -f /tmp/splice-saved.bin

echo ""
echo "✅ All text operation tests passed!"
