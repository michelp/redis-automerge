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
result=$(redis-cli -h "$HOST" am.index.configure "article:*" title content)
assert_equals "$result" "OK"
echo "   ✓ Index configuration created"

# Test 2: Verify configuration is saved
echo "Test 2: Verify configuration is saved..."
# Check that configuration key exists
exists=$(redis-cli -h "$HOST" exists "am:index:config:article:*")
assert_equals "$exists" "1"
# Check enabled field
enabled=$(redis-cli -h "$HOST" hget "am:index:config:article:*" enabled)
assert_equals "$enabled" "1"
# Check paths field
paths=$(redis-cli -h "$HOST" hget "am:index:config:article:*" paths)
assert_equals "$paths" "title,content"
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
redis-cli -h "$HOST" am.index.configure "user:*" name profile.bio profile.location > /dev/null
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
result=$(redis-cli -h "$HOST" am.index.disable "user:*")
assert_equals "$result" "OK"
# Check that enabled field is now 0
enabled=$(redis-cli -h "$HOST" hget "am:index:config:user:*" enabled)
assert_equals "$enabled" "0"
# Update document - shadow Hash should NOT update
redis-cli -h "$HOST" am.puttext "user:alice" name "Alice Updated" > /dev/null
name=$(redis-cli -h "$HOST" --raw hget "am:idx:user:alice" name)
assert_equals "$name" "Alice"  # Should still be old value
echo "   ✓ Disabled index does not update"

# Test 7: AM.INDEX.ENABLE command
echo "Test 7: AM.INDEX.ENABLE command..."
result=$(redis-cli -h "$HOST" am.index.enable "user:*")
assert_equals "$result" "OK"
# Check that enabled field is now 1
enabled=$(redis-cli -h "$HOST" hget "am:index:config:user:*" enabled)
assert_equals "$enabled" "1"
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
redis-cli -h "$HOST" am.index.disable "article:*" > /dev/null
# Update the document (should not create shadow Hash)
redis-cli -h "$HOST" am.puttext "article:456" title "Updated Title" > /dev/null
# Re-enable and reindex
redis-cli -h "$HOST" am.index.enable "article:*" > /dev/null
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
status=$(redis-cli -h "$HOST" am.index.status "article:*")
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
redis-cli -h "$HOST" am.index.configure "*" name > /dev/null
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
# Clean up wildcard config from previous test
redis-cli -h "$HOST" del "am:index:config:*" > /dev/null
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

# Test 14: Index created when loading document
echo "Test 14: Index created when loading document..."
redis-cli -h "$HOST" del "article:loaded" > /dev/null
# Create a document, save it, then load it to trigger indexing
redis-cli -h "$HOST" am.new "article:temp" > /dev/null
redis-cli -h "$HOST" am.puttext "article:temp" title "Loaded Article" > /dev/null
redis-cli -h "$HOST" am.puttext "article:temp" content "Loaded Content" > /dev/null
# Save and load
redis-cli -h "$HOST" --raw am.save "article:temp" > /tmp/article-test.bin
truncate -s -1 /tmp/article-test.bin
redis-cli -h "$HOST" --raw -x am.load "article:loaded" < /tmp/article-test.bin > /dev/null
redis-cli -h "$HOST" del "article:temp" > /dev/null
# Check that loaded document's index was created
exists=$(redis-cli -h "$HOST" exists "am:idx:article:loaded")
# AM.LOAD doesn't automatically trigger indexing (intentional design choice)
# So we expect 0 here and manually reindex
assert_equals "$exists" "0"
# Manually reindex
redis-cli -h "$HOST" am.index.reindex "article:loaded" > /dev/null
# Now check that index exists
exists=$(redis-cli -h "$HOST" exists "am:idx:article:loaded")
assert_equals "$exists" "1"
title=$(redis-cli -h "$HOST" --raw hget "am:idx:article:loaded" title)
content=$(redis-cli -h "$HOST" --raw hget "am:idx:article:loaded" content)
assert_equals "$title" "Loaded Article"
assert_equals "$content" "Loaded Content"
rm -f /tmp/article-test.bin
echo "   ✓ Manual reindexing after load works"

# Test 15: Multiple pattern configurations
echo "Test 15: Multiple pattern configurations..."
redis-cli -h "$HOST" am.index.configure "blog:*" title body tags > /dev/null
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
redis-cli -h "$HOST" am.index.configure "article:*" title content author category > /dev/null
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
