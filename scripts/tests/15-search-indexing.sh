#!/usr/bin/env bash
# Test search indexing functionality

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Search Indexing"

# Test 1: Configure indexing for a pattern
echo "Test 1: Configure indexing for a pattern..."
redis-cli -h "$HOST" del searchdoc1 > /dev/null
redis-cli -h "$HOST" am.new searchdoc1 > /dev/null
# Configure indexing for article:* pattern with title and content paths
result=$(redis-cli -h "$HOST" am.index.configure am:index:configs "article:*" title content)
assert_equals "$result" "OK"
echo "   ✓ Index configuration created"

# Test 2: Verify configuration is saved
echo "Test 2: Verify configuration is saved..."
# All configs now live in a single Hash (audit #12): key=am:index:configs,
# field=<pattern>, value=JSON-serialized {enabled, paths, format}.
exists=$(redis-cli -h "$HOST" exists "am:index:configs")
assert_equals "$exists" "1"
serialized=$(redis-cli -h "$HOST" --raw hget "am:index:configs" "article:*")
echo "$serialized" | grep -q '"enabled":true'
echo "$serialized" | grep -q '"title"'
echo "$serialized" | grep -q '"content"'
echo "   ✓ Configuration persisted correctly"

# Test 3: Automatic index creation on PUTTEXT
echo "Test 3: Automatic index creation on PUTTEXT..."
redis-cli -h "$HOST" del "article:123" > /dev/null
redis-cli -h "$HOST" am.new "article:123" > /dev/null
redis-cli -h "$HOST" am.puttext "article:123" title "Redis and Automerge" > /dev/null
redis-cli -h "$HOST" am.puttext "article:123" content "A guide to CRDTs in Redis" > /dev/null
redis-cli -h "$HOST" am.puttext "article:123" author "John Doe" > /dev/null
# Check that shadow Hash was created
exists=$(redis-cli -h "$HOST" exists "am:idx:article:123")
assert_equals "$exists" "1"
# Check indexed fields are present
title=$(redis-cli -h "$HOST" --raw hget "am:idx:article:123" title)
content=$(redis-cli -h "$HOST" --raw hget "am:idx:article:123" content)
assert_equals "$title" "Redis and Automerge"
assert_equals "$content" "A guide to CRDTs in Redis"
# Check that non-configured field (author) is NOT indexed
author=$(redis-cli -h "$HOST" hget "am:idx:article:123" author)
assert_equals "$author" ""
echo "   ✓ Shadow Hash created with configured fields only"

# Test 4: Automatic index update on field modification
echo "Test 4: Automatic index update on field modification..."
redis-cli -h "$HOST" am.puttext "article:123" title "Updated Title" > /dev/null
title=$(redis-cli -h "$HOST" --raw hget "am:idx:article:123" title)
assert_equals "$title" "Updated Title"
echo "   ✓ Shadow Hash updated on field modification"

# Test 5: Nested path indexing
echo "Test 5: Nested path indexing..."
redis-cli -h "$HOST" am.index.configure am:index:configs "user:*" name profile.bio profile.location > /dev/null
redis-cli -h "$HOST" del "user:alice" > /dev/null
redis-cli -h "$HOST" am.new "user:alice" > /dev/null
redis-cli -h "$HOST" am.puttext "user:alice" name "Alice" > /dev/null
redis-cli -h "$HOST" am.puttext "user:alice" profile.bio "Software engineer" > /dev/null
redis-cli -h "$HOST" am.puttext "user:alice" profile.location "San Francisco" > /dev/null
# Check shadow Hash
exists=$(redis-cli -h "$HOST" exists "am:idx:user:alice")
assert_equals "$exists" "1"
name=$(redis-cli -h "$HOST" --raw hget "am:idx:user:alice" name)
bio=$(redis-cli -h "$HOST" --raw hget "am:idx:user:alice" profile_bio)
location=$(redis-cli -h "$HOST" --raw hget "am:idx:user:alice" profile_location)
assert_equals "$name" "Alice"
assert_equals "$bio" "Software engineer"
assert_equals "$location" "San Francisco"
echo "   ✓ Nested paths indexed correctly (dots replaced with underscores)"

# Test 6: AM.INDEX.DISABLE command
echo "Test 6: AM.INDEX.DISABLE command..."
result=$(redis-cli -h "$HOST" am.index.disable am:index:configs "user:*")
assert_equals "$result" "OK"
# Confirm enabled flipped to false in the serialized config (audit #12).
serialized=$(redis-cli -h "$HOST" --raw hget "am:index:configs" "user:*")
echo "$serialized" | grep -q '"enabled":false'
# Update document - shadow Hash should NOT update
redis-cli -h "$HOST" am.puttext "user:alice" name "Alice Updated" > /dev/null
name=$(redis-cli -h "$HOST" --raw hget "am:idx:user:alice" name)
assert_equals "$name" "Alice"  # Should still be old value
echo "   ✓ Disabled index does not update"

# Test 7: AM.INDEX.ENABLE command
echo "Test 7: AM.INDEX.ENABLE command..."
result=$(redis-cli -h "$HOST" am.index.enable am:index:configs "user:*")
assert_equals "$result" "OK"
serialized=$(redis-cli -h "$HOST" --raw hget "am:index:configs" "user:*")
echo "$serialized" | grep -q '"enabled":true'
# Update document - shadow Hash should now update
redis-cli -h "$HOST" am.puttext "user:alice" name "Alice Re-enabled" > /dev/null
name=$(redis-cli -h "$HOST" --raw hget "am:idx:user:alice" name)
assert_equals "$name" "Alice Re-enabled"
echo "   ✓ Re-enabled index updates correctly"

# Test 8: AM.INDEX.REINDEX command
echo "Test 8: AM.INDEX.REINDEX command..."
# Create document without index, then configure and reindex
redis-cli -h "$HOST" del "article:456" > /dev/null
redis-cli -h "$HOST" am.new "article:456" > /dev/null
redis-cli -h "$HOST" am.puttext "article:456" title "Pre-index Article" > /dev/null
redis-cli -h "$HOST" am.puttext "article:456" content "Created before indexing" > /dev/null
# Disable indexing temporarily
redis-cli -h "$HOST" am.index.disable am:index:configs "article:*" > /dev/null
# Update the document (should not create shadow Hash)
redis-cli -h "$HOST" am.puttext "article:456" title "Updated Title" > /dev/null
# Re-enable and reindex
redis-cli -h "$HOST" am.index.enable am:index:configs "article:*" > /dev/null
result=$(redis-cli -h "$HOST" am.index.reindex "article:456")
assert_equals "$result" "1"
# Check shadow Hash has current values
title=$(redis-cli -h "$HOST" --raw hget "am:idx:article:456" title)
content=$(redis-cli -h "$HOST" --raw hget "am:idx:article:456" content)
assert_equals "$title" "Updated Title"
assert_equals "$content" "Created before indexing"
echo "   ✓ REINDEX command rebuilds shadow Hash"

# Test 9: AM.INDEX.STATUS command
echo "Test 9: AM.INDEX.STATUS command..."
status=$(redis-cli -h "$HOST" am.index.status am:index:configs "article:*")
# Status should contain pattern, enabled, and paths
echo "$status" | grep -q "article:\*"
echo "$status" | grep -q "enabled"
echo "$status" | grep -q "title"
echo "$status" | grep -q "content"
echo "   ✓ STATUS command returns configuration"

# Test 10: Non-matching key does not create index
echo "Test 10: Non-matching key does not create index..."
redis-cli -h "$HOST" del "post:789" > /dev/null
redis-cli -h "$HOST" am.new "post:789" > /dev/null
redis-cli -h "$HOST" am.puttext "post:789" title "Post Title" > /dev/null
# Should not create shadow Hash (no config for post:* pattern)
exists=$(redis-cli -h "$HOST" exists "am:idx:post:789")
assert_equals "$exists" "0"
echo "   ✓ Non-matching patterns do not create indexes"

# Test 11: FROMJSON automatic indexing
echo "Test 11: FROMJSON automatic indexing..."
redis-cli -h "$HOST" del "article:789" > /dev/null
redis-cli -h "$HOST" am.fromjson "article:789" '{"title":"JSON Article","content":"Created from JSON","author":"Jane"}' > /dev/null
# Check shadow Hash
exists=$(redis-cli -h "$HOST" exists "am:idx:article:789")
assert_equals "$exists" "1"
title=$(redis-cli -h "$HOST" --raw hget "am:idx:article:789" title)
content=$(redis-cli -h "$HOST" --raw hget "am:idx:article:789" content)
assert_equals "$title" "JSON Article"
assert_equals "$content" "Created from JSON"
# Author should not be indexed
author=$(redis-cli -h "$HOST" hget "am:idx:article:789" author)
assert_equals "$author" ""
echo "   ✓ FROMJSON triggers automatic indexing"

# Test 12: Wildcard pattern matching
echo "Test 12: Wildcard pattern matching..."
# Configure indexing for any key
redis-cli -h "$HOST" am.index.configure am:index:configs "*" name > /dev/null
redis-cli -h "$HOST" del "anykey" > /dev/null
redis-cli -h "$HOST" am.new "anykey" > /dev/null
redis-cli -h "$HOST" am.puttext "anykey" name "Wildcard Match" > /dev/null
# Check shadow Hash
exists=$(redis-cli -h "$HOST" exists "am:idx:anykey")
assert_equals "$exists" "1"
name=$(redis-cli -h "$HOST" --raw hget "am:idx:anykey" name)
assert_equals "$name" "Wildcard Match"
echo "   ✓ Wildcard pattern (*) matches all keys"

# Test 13: Empty fields not indexed
echo "Test 13: Empty fields not indexed..."
# Clean up wildcard config from previous test using AM.INDEX.DELETE so the
# in-memory config cache is invalidated alongside the underlying Redis Hash.
redis-cli -h "$HOST" am.index.delete am:index:configs "*" > /dev/null
redis-cli -h "$HOST" del "article:999" > /dev/null
redis-cli -h "$HOST" am.new "article:999" > /dev/null
redis-cli -h "$HOST" am.puttext "article:999" title "Only Title" > /dev/null
# content is not set, so shadow Hash should only have title
exists=$(redis-cli -h "$HOST" exists "am:idx:article:999")
assert_equals "$exists" "1"
title=$(redis-cli -h "$HOST" --raw hget "am:idx:article:999" title)
assert_equals "$title" "Only Title"
# content field should not exist in Hash
content=$(redis-cli -h "$HOST" hget "am:idx:article:999" content)
assert_equals "$content" ""
echo "   ✓ Empty configured fields not indexed"

# Test 14: AM.LOAD triggers automatic indexing (audit #11 fix)
echo "Test 14: AM.LOAD triggers automatic indexing..."
redis-cli -h "$HOST" del "article:loaded" > /dev/null
# Create a document, save it, then load it
redis-cli -h "$HOST" am.new "article:temp" > /dev/null
redis-cli -h "$HOST" am.puttext "article:temp" title "Loaded Article" > /dev/null
redis-cli -h "$HOST" am.puttext "article:temp" content "Loaded Content" > /dev/null
redis-cli -h "$HOST" --raw am.save "article:temp" > /tmp/article-test.bin
truncate -s -1 /tmp/article-test.bin
redis-cli -h "$HOST" --raw -x am.load "article:loaded" < /tmp/article-test.bin > /dev/null
redis-cli -h "$HOST" del "article:temp" > /dev/null
# AM.LOAD now centralizes through finalize_write_meta, which updates the
# search shadow index. The shadow Hash should exist immediately after LOAD
# without a separate AM.INDEX.REINDEX call.
exists=$(redis-cli -h "$HOST" exists "am:idx:article:loaded")
assert_equals "$exists" "1"
title=$(redis-cli -h "$HOST" --raw hget "am:idx:article:loaded" title)
content=$(redis-cli -h "$HOST" --raw hget "am:idx:article:loaded" content)
assert_equals "$title" "Loaded Article"
assert_equals "$content" "Loaded Content"
rm -f /tmp/article-test.bin
echo "   ✓ AM.LOAD automatically populates shadow index"

# Test 15: Multiple pattern configurations
echo "Test 15: Multiple pattern configurations..."
redis-cli -h "$HOST" am.index.configure am:index:configs "blog:*" title body tags > /dev/null
redis-cli -h "$HOST" del "blog:post1" > /dev/null
redis-cli -h "$HOST" am.new "blog:post1" > /dev/null
redis-cli -h "$HOST" am.puttext "blog:post1" title "Blog Post" > /dev/null
redis-cli -h "$HOST" am.puttext "blog:post1" body "Blog body text" > /dev/null
# Check shadow Hash
exists=$(redis-cli -h "$HOST" exists "am:idx:blog:post1")
assert_equals "$exists" "1"
title=$(redis-cli -h "$HOST" --raw hget "am:idx:blog:post1" title)
body=$(redis-cli -h "$HOST" --raw hget "am:idx:blog:post1" body)
assert_equals "$title" "Blog Post"
assert_equals "$body" "Blog body text"
# Original article:* pattern should still work
redis-cli -h "$HOST" am.puttext "article:123" title "Still Works" > /dev/null
title=$(redis-cli -h "$HOST" --raw hget "am:idx:article:123" title)
assert_equals "$title" "Still Works"
echo "   ✓ Multiple patterns can coexist"

# Test 15b: Audit #11 regression — non-PUTTEXT writes that mutate indexed
# text fields must trigger the shadow-index update. Before the audit-#11 fix,
# only AM.PUTTEXT / AM.APPLY / AM.FROMJSON updated the shadow Hash; other
# commands silently desynced it. The Hash indexer only stores text fields, so
# this test mutates text via SPLICETEXT and PUTDIFF (which previously did not
# trigger indexing) and asserts the shadow Hash reflects the new content.
echo "Test 15b: Text mutations via non-PUTTEXT commands update the index..."
redis-cli -h "$HOST" am.index.configure am:index:configs "audit11:*" body > /dev/null
redis-cli -h "$HOST" del "audit11:doc" > /dev/null
redis-cli -h "$HOST" am.new "audit11:doc" > /dev/null
redis-cli -h "$HOST" am.puttext "audit11:doc" body "hello world" > /dev/null
v=$(redis-cli -h "$HOST" --raw hget "am:idx:audit11:doc" body)
assert_equals "$v" "hello world"

# AM.SPLICETEXT — replace "world" with "audit": pos=6 del=5 insert="audit"
redis-cli -h "$HOST" am.splicetext "audit11:doc" body 6 5 "audit" > /dev/null
v=$(redis-cli -h "$HOST" --raw hget "am:idx:audit11:doc" body)
assert_equals "$v" "hello audit"

# AM.PUTDIFF — apply a unified diff that overwrites the text
diff_payload=$'--- a\n+++ b\n@@ -1 +1 @@\n-hello audit\n+hello diff\n'
redis-cli -h "$HOST" am.putdiff "audit11:doc" body "$diff_payload" > /dev/null
v=$(redis-cli -h "$HOST" --raw hget "am:idx:audit11:doc" body)
assert_equals "$v" "hello diff"

# AM.MARKCREATE — adding a mark doesn't change the text content but must
# still leave the indexed value intact (shadow not corrupted by the trigger).
redis-cli -h "$HOST" am.markcreate "audit11:doc" body bold bool true 0 5 > /dev/null
v=$(redis-cli -h "$HOST" --raw hget "am:idx:audit11:doc" body)
assert_equals "$v" "hello diff"

echo "   ✓ SPLICETEXT/PUTDIFF/MARKCREATE keep the shadow index in sync"

# Test 15c: Audit #12 regression — admin commands reject a store-key that
# does not match the module-load-configured index-config-key. This is what
# makes the keyspec/ACL/cluster fix safe: a misrouted call would otherwise
# silently land in a Hash the runtime indexer never reads.
echo "Test 15c: Wrong store-key is rejected..."
mismatch=$(redis-cli -h "$HOST" am.index.configure am:index:wrong "audit12:*" body 2>&1)
echo "$mismatch" | grep -qi "store-key must match"
mismatch=$(redis-cli -h "$HOST" am.index.status am:index:wrong 2>&1)
echo "$mismatch" | grep -qi "store-key must match"
mismatch=$(redis-cli -h "$HOST" am.index.delete am:index:wrong "audit12:*" 2>&1)
echo "$mismatch" | grep -qi "store-key must match"
echo "   ✓ Mismatched store-key returns an explicit error"

# Test 15d: Audit #13 regression — the indexer must not clobber a user key
# that happens to live at am:idx:<name>. The shadow-ownership check (sentinel
# field __am_idx__) refuses to overwrite/delete keys it does not own and
# preserves the user's original data unchanged.
echo "Test 15d: Pre-existing user Hash at shadow path is preserved..."
redis-cli -h "$HOST" am.index.configure am:index:configs "collision:*" body > /dev/null
# User stashes data at exactly the shadow path the indexer would use.
redis-cli -h "$HOST" del "am:idx:collision:doc" > /dev/null
redis-cli -h "$HOST" hset "am:idx:collision:doc" mine "user-content" > /dev/null
# Now create the AM doc that would match the index pattern.
redis-cli -h "$HOST" del "collision:doc" > /dev/null
redis-cli -h "$HOST" am.new "collision:doc" > /dev/null
result=$(redis-cli -h "$HOST" am.puttext "collision:doc" body "from automerge" 2>&1)
# AM.PUTTEXT itself succeeds (indexer errors are best-effort, see
# try_update_search_index in lib.rs).
assert_equals "$result" "OK"
# But the user's original Hash field MUST still be intact.
mine=$(redis-cli -h "$HOST" --raw hget "am:idx:collision:doc" mine)
assert_equals "$mine" "user-content"
# And the indexer must NOT have written its own field.
indexed=$(redis-cli -h "$HOST" --raw hget "am:idx:collision:doc" body)
assert_equals "$indexed" ""
# No sentinel was added either.
sentinel=$(redis-cli -h "$HOST" --raw hget "am:idx:collision:doc" __am_idx__)
assert_equals "$sentinel" ""
# Cleanup.
redis-cli -h "$HOST" del "am:idx:collision:doc" "collision:doc" > /dev/null
redis-cli -h "$HOST" am.index.delete am:index:configs "collision:*" > /dev/null
echo "   ✓ User Hash at am:idx:collision:doc preserved; indexer skipped"

# Test 15e: With no pre-existing collision the indexer creates a properly
# stamped shadow, including the __am_idx__ sentinel.
echo "Test 15e: Owned shadow carries the sentinel..."
redis-cli -h "$HOST" am.index.configure am:index:configs "owned:*" title > /dev/null
redis-cli -h "$HOST" del "am:idx:owned:doc" "owned:doc" > /dev/null
redis-cli -h "$HOST" am.new "owned:doc" > /dev/null
redis-cli -h "$HOST" am.puttext "owned:doc" title "Hello" > /dev/null
sentinel=$(redis-cli -h "$HOST" --raw hget "am:idx:owned:doc" __am_idx__)
assert_equals "$sentinel" "owned:doc"
title=$(redis-cli -h "$HOST" --raw hget "am:idx:owned:doc" title)
assert_equals "$title" "Hello"
# A subsequent write should keep working since we own the shadow.
redis-cli -h "$HOST" am.puttext "owned:doc" title "Hello again" > /dev/null
title=$(redis-cli -h "$HOST" --raw hget "am:idx:owned:doc" title)
assert_equals "$title" "Hello again"
redis-cli -h "$HOST" del "am:idx:owned:doc" "owned:doc" > /dev/null
redis-cli -h "$HOST" am.index.delete am:index:configs "owned:*" > /dev/null
echo "   ✓ Sentinel is set on creation and updates pass through"

# Test 15f: Audit #17 regression — non-UTF8 keys must not collide via the
# lossy `RedisString::to_string()` conversion. The indexer now skips
# documents whose key is not valid UTF-8 so two distinct binary keys can
# no longer share a shadow path. We send raw RESP via bash's /dev/tcp
# because redis-cli only accepts binary at the last arg position.
echo "Test 15f: Non-UTF8 key skips indexing (no collision)..."
redis-cli -h "$HOST" am.index.configure am:index:configs "*" title > /dev/null

# Helper: send a single RESP command of N bulk-string args via /dev/tcp.
# Args are passed through `printf '%b'` so \xNN escapes are honored.
send_resp_binary() {
    exec 3<>"/dev/tcp/$HOST/6379"
    local n=$#
    printf '*%d\r\n' "$n" >&3
    for arg in "$@"; do
        # Resolve %b escapes to actual bytes, count them, write the bulk header
        # and the bytes verbatim. We use a temp file because $() strips NULs
        # (we don't actually use NUL but principled).
        local tmp
        tmp=$(mktemp)
        printf '%b' "$arg" > "$tmp"
        local len
        len=$(wc -c < "$tmp")
        printf '$%d\r\n' "$len" >&3
        cat "$tmp" >&3
        printf '\r\n' >&3
        rm -f "$tmp"
    done
    # Drain reply so the server doesn't block on a half-closed pipe.
    timeout 1 cat <&3 > /dev/null 2>&1 || true
    exec 3<&-
    exec 3>&-
}

# Two distinct binary keys that would collapse to the same U+FFFD-replaced
# UTF-8 string under the old to_string() path: one ends in \xff\xfe, the
# other in \xff\xfd. Both start with the printable prefix `binkey:`.
send_resp_binary 'AM.NEW' 'binkey:\xff\xfe'
send_resp_binary 'AM.PUTTEXT' 'binkey:\xff\xfe' 'title' 'A'
send_resp_binary 'AM.NEW' 'binkey:\xff\xfd'
send_resp_binary 'AM.PUTTEXT' 'binkey:\xff\xfd' 'title' 'B'

# No shadow under the binkey: prefix should exist — indexing was skipped.
shadows=$(redis-cli -h "$HOST" --scan --pattern 'am:idx:binkey:*' | wc -l)
assert_equals "$shadows" "0"

# Cleanup: DEL the binary AM keys.
send_resp_binary 'DEL' 'binkey:\xff\xfe'
send_resp_binary 'DEL' 'binkey:\xff\xfd'
redis-cli -h "$HOST" am.index.delete am:index:configs "*" > /dev/null
echo "   ✓ Non-UTF8 keys skip indexing; no shadow collision"

# Test 15g: Audit #25 — patterns with Redis-glob metacharacters that the
# in-tree matcher does not implement must be rejected at configure-time,
# so misconfigurations fail loudly rather than silently never matching.
echo "Test 15g: Unsupported glob metacharacters are rejected..."
for bad in 'user?' 'user[12]:*' 'tag\:*' ''; do
    out=$(redis-cli -h "$HOST" am.index.configure am:index:configs "$bad" title 2>&1)
    if echo "$out" | grep -qi 'error\|unsupported\|must not be empty'; then
        :
    else
        echo "   ✗ pattern '$bad' was not rejected (got: $out)"
        exit 1
    fi
done
# Same gate must apply to ENABLE's auto-create branch.
out=$(redis-cli -h "$HOST" am.index.enable am:index:configs 'user[12]:*' 2>&1)
echo "$out" | grep -qi 'error\|unsupported' || {
    echo "   ✗ ENABLE accepted unsupported pattern (got: $out)"
    exit 1
}
# DELETE remains permissive so users can clean up any stale invalid entries.
redis-cli -h "$HOST" am.index.delete am:index:configs 'user[12]:*' > /dev/null
# Valid patterns still pass.
result=$(redis-cli -h "$HOST" am.index.configure am:index:configs "valid:*" title)
assert_equals "$result" "OK"
redis-cli -h "$HOST" am.index.delete am:index:configs "valid:*" > /dev/null
echo "   ✓ Unsupported glob metacharacters rejected at configure/enable"

# Test 15h: Audit #28 — AM.INDEX.CONFIGURE option grammar is tight.
# `--format` only in slot 3, missing value rejected, option-like paths
# rejected unless `--` precedes them.
echo "Test 15h: --format / -- option-grammar is explicit..."

# Missing value after --format
out=$(redis-cli -h "$HOST" am.index.configure am:index:configs "fmt:*" --format 2>&1)
echo "$out" | grep -qi "requires a value" || {
    echo "   ✗ missing --format value not rejected (got: $out)"; exit 1; }

# Bad format value
out=$(redis-cli -h "$HOST" am.index.configure am:index:configs "fmt:*" --format yaml title 2>&1)
echo "$out" | grep -qi "Invalid format" || {
    echo "   ✗ invalid --format value not rejected (got: $out)"; exit 1; }

# Typo'd option-like path rejected
out=$(redis-cli -h "$HOST" am.index.configure am:index:configs "fmt:*" --foramt hash title 2>&1)
echo "$out" | grep -qi "unexpected option-like path" || {
    echo "   ✗ --foramt typo not rejected (got: $out)"; exit 1; }

# Option-like path after `--` is accepted as a literal path
result=$(redis-cli -h "$HOST" am.index.configure am:index:configs "term:*" -- --weird-path)
assert_equals "$result" "OK"
serialized=$(redis-cli -h "$HOST" --raw hget "am:index:configs" "term:*")
echo "$serialized" | grep -q '"--weird-path"' || {
    echo "   ✗ '--' did not preserve literal path (got: $serialized)"; exit 1; }
redis-cli -h "$HOST" am.index.delete am:index:configs "term:*" > /dev/null

# --format json with -- still works
result=$(redis-cli -h "$HOST" am.index.configure am:index:configs "both:*" --format json -- title)
assert_equals "$result" "OK"
serialized=$(redis-cli -h "$HOST" --raw hget "am:index:configs" "both:*")
echo "$serialized" | grep -q '"format":"json"' || {
    echo "   ✗ --format json with -- not honored (got: $serialized)"; exit 1; }
redis-cli -h "$HOST" am.index.delete am:index:configs "both:*" > /dev/null

# Normal --format hash path still works
result=$(redis-cli -h "$HOST" am.index.configure am:index:configs "norm:*" --format hash title)
assert_equals "$result" "OK"
redis-cli -h "$HOST" am.index.delete am:index:configs "norm:*" > /dev/null

echo "   ✓ --format / -- grammar is tight; typos and missing values rejected"

# Test 16: FT.CREATE and FT.SEARCH integration - Text search
echo "Test 16: RediSearch integration - Full-text search..."
# Check if RediSearch is available
if ! redis-cli -h "$HOST" module list 2>/dev/null | grep -q "search"; then
    echo "   ⚠️  RediSearch module not found - skipping FT.SEARCH tests"
    echo "   To run these tests, install RediSearch: https://redis.io/docs/stack/search/"
    echo ""
    echo "✅ All search indexing tests passed (FT.SEARCH tests skipped)!"
    exit 0
fi
# Clean up any existing index
redis-cli -h "$HOST" ft.dropindex idx:test_articles 2>/dev/null || true
# Create fresh test articles
redis-cli -h "$HOST" del "article:search1" "article:search2" "article:search3" "article:search4" > /dev/null
redis-cli -h "$HOST" am.index.configure am:index:configs "article:*" title content author category > /dev/null
redis-cli -h "$HOST" am.new "article:search1" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search1" title "Redis CRDT Tutorial" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search1" content "Learn about Conflict-free Replicated Data Types in Redis" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search1" author "Alice" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search1" category "tutorial" > /dev/null
redis-cli -h "$HOST" am.new "article:search2" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search2" title "Advanced Automerge Techniques" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search2" content "Deep dive into CRDT algorithms and Automerge internals" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search2" author "Bob" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search2" category "advanced" > /dev/null
redis-cli -h "$HOST" am.new "article:search3" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search3" title "Redis Performance Tips" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search3" content "Optimize your Redis deployment for maximum performance" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search3" author "Charlie" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search3" category "performance" > /dev/null
redis-cli -h "$HOST" am.new "article:search4" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search4" title "Database Sharding Strategies" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search4" content "Horizontal scaling with database partitioning" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search4" author "Alice" > /dev/null
redis-cli -h "$HOST" am.puttext "article:search4" category "architecture" > /dev/null
# Create RediSearch index
redis-cli -h "$HOST" ft.create idx:test_articles on hash prefix 1 am:idx:article:search schema title text content text author text category tag > /dev/null
# Search for "CRDT" - should find search1 and search2
results=$(redis-cli -h "$HOST" ft.search idx:test_articles "CRDT" nocontent)
echo "$results" | grep -q "am:idx:article:search1"
echo "$results" | grep -q "am:idx:article:search2"
# Verify search3 and search4 are NOT in results (no CRDT mention)
if echo "$results" | grep -q "am:idx:article:search3"; then
    echo "   ✗ Search incorrectly included search3"
    exit 1
fi
if echo "$results" | grep -q "am:idx:article:search4"; then
    echo "   ✗ Search incorrectly included search4"
    exit 1
fi
echo "   ✓ Full-text search correctly finds and excludes documents"

# Test 17: FT.SEARCH - Field-specific search
echo "Test 17: RediSearch integration - Field-specific search..."
# Search by author:Alice - should find search1 and search4
results=$(redis-cli -h "$HOST" ft.search idx:test_articles "@author:Alice" nocontent)
echo "$results" | grep -q "am:idx:article:search1"
echo "$results" | grep -q "am:idx:article:search4"
# Verify Bob and Charlie articles are excluded
if echo "$results" | grep -q "am:idx:article:search2"; then
    echo "   ✗ Author search incorrectly included search2 (Bob)"
    exit 1
fi
if echo "$results" | grep -q "am:idx:article:search3"; then
    echo "   ✗ Author search incorrectly included search3 (Charlie)"
    exit 1
fi
echo "   ✓ Field-specific search correctly filters by author"

# Test 18: FT.SEARCH - Tag search
echo "Test 18: RediSearch integration - Tag search..."
# Search by category:tutorial - should only find search1
results=$(redis-cli -h "$HOST" ft.search idx:test_articles "@category:{tutorial}" nocontent)
echo "$results" | grep -q "am:idx:article:search1"
# Verify other categories are excluded
if echo "$results" | grep -q "am:idx:article:search2"; then
    echo "   ✗ Tag search incorrectly included search2 (advanced)"
    exit 1
fi
if echo "$results" | grep -q "am:idx:article:search3"; then
    echo "   ✗ Tag search incorrectly included search3 (performance)"
    exit 1
fi
if echo "$results" | grep -q "am:idx:article:search4"; then
    echo "   ✗ Tag search incorrectly included search4 (architecture)"
    exit 1
fi
echo "   ✓ Tag search correctly filters by category"

# Test 19: FT.SEARCH - Combined query
echo "Test 19: RediSearch integration - Combined query..."
# Search for Redis articles by Alice - should only find search1
results=$(redis-cli -h "$HOST" ft.search idx:test_articles "@author:Alice Redis" nocontent)
echo "$results" | grep -q "am:idx:article:search1"
# search4 by Alice but no Redis mention - should be excluded
if echo "$results" | grep -q "am:idx:article:search4"; then
    echo "   ✗ Combined query incorrectly included search4 (no Redis mention)"
    exit 1
fi
# search3 has Redis but wrong author - should be excluded
if echo "$results" | grep -q "am:idx:article:search3"; then
    echo "   ✗ Combined query incorrectly included search3 (wrong author)"
    exit 1
fi
echo "   ✓ Combined query correctly filters by multiple criteria"

# Test 20: FT.SEARCH - Title-specific search
echo "Test 20: RediSearch integration - Title field search..."
# Search for "Performance" in title - should only find search3
results=$(redis-cli -h "$HOST" ft.search idx:test_articles "@title:Performance" nocontent)
echo "$results" | grep -q "am:idx:article:search3"
# Verify others are excluded
if echo "$results" | grep -q "am:idx:article:search1"; then
    echo "   ✗ Title search incorrectly included search1"
    exit 1
fi
if echo "$results" | grep -q "am:idx:article:search2"; then
    echo "   ✗ Title search incorrectly included search2"
    exit 1
fi
if echo "$results" | grep -q "am:idx:article:search4"; then
    echo "   ✗ Title search incorrectly included search4"
    exit 1
fi
echo "   ✓ Title field search works correctly"

# Cleanup
redis-cli -h "$HOST" ft.dropindex idx:test_articles > /dev/null 2>&1 || true

echo ""
echo "✅ All search indexing tests passed!"
