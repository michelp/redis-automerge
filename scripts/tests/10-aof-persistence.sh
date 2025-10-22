#!/usr/bin/env bash
# Test AOF persistence and Redis restart scenarios

set -euo pipefail

# Load common test utilities
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "AOF Persistence & Restart"

echo "Test 1: Basic AOF persistence after restart..."
redis-cli -h "$HOST" del persist_test1 > /dev/null
redis-cli -h "$HOST" am.new persist_test1 > /dev/null
redis-cli -h "$HOST" am.puttext persist_test1 message "Persisted data" > /dev/null
redis-cli -h "$HOST" am.putint persist_test1 count 99 > /dev/null

# Verify data before restart
val=$(redis-cli -h "$HOST" --raw am.gettext persist_test1 message)
assert_equals "$val" "Persisted data"

# Try to restart if in Docker environment
if restart_redis; then
    # Verify data after restart
    val=$(redis-cli -h "$HOST" --raw am.gettext persist_test1 message)
    count=$(redis-cli -h "$HOST" am.getint persist_test1 count)

    if [ "$val" = "Persisted data" ] && [ "$count" = "99" ]; then
        echo "   ✓ Documents survive Redis restart"
    else
        echo "   ✗ Data lost after restart (text='$val', count='$count')"
        exit 1
    fi
else
    echo "   ⚠ Skipped restart test (Docker not available)"
fi

echo "Test 2: Multiple documents persist after restart..."
redis-cli -h "$HOST" del persist_doc1 persist_doc2 persist_doc3 > /dev/null

# Create multiple documents with different data
redis-cli -h "$HOST" am.new persist_doc1 > /dev/null
redis-cli -h "$HOST" am.puttext persist_doc1 title "Document 1" > /dev/null
redis-cli -h "$HOST" am.putint persist_doc1 version 1 > /dev/null

redis-cli -h "$HOST" am.new persist_doc2 > /dev/null
redis-cli -h "$HOST" am.puttext persist_doc2 title "Document 2" > /dev/null
redis-cli -h "$HOST" am.putbool persist_doc2 published true > /dev/null

redis-cli -h "$HOST" am.new persist_doc3 > /dev/null
redis-cli -h "$HOST" am.createlist persist_doc3 items > /dev/null
redis-cli -h "$HOST" am.appendtext persist_doc3 items "first" > /dev/null
redis-cli -h "$HOST" am.appendtext persist_doc3 items "second" > /dev/null

# Try to restart if in Docker environment
if restart_redis; then
    # Verify all documents after restart
    title1=$(redis-cli -h "$HOST" --raw am.gettext persist_doc1 title)
    version1=$(redis-cli -h "$HOST" am.getint persist_doc1 version)

    title2=$(redis-cli -h "$HOST" --raw am.gettext persist_doc2 title)
    published2=$(redis-cli -h "$HOST" am.getbool persist_doc2 published)

    len3=$(redis-cli -h "$HOST" am.listlen persist_doc3 items)
    item1=$(redis-cli -h "$HOST" --raw am.gettext persist_doc3 'items[0]')
    item2=$(redis-cli -h "$HOST" --raw am.gettext persist_doc3 'items[1]')

    errors=0
    [ "$title1" != "Document 1" ] && { echo "   ✗ Doc1 title wrong: $title1"; errors=$((errors+1)); }
    [ "$version1" != "1" ] && { echo "   ✗ Doc1 version wrong: $version1"; errors=$((errors+1)); }
    [ "$title2" != "Document 2" ] && { echo "   ✗ Doc2 title wrong: $title2"; errors=$((errors+1)); }
    [ "$published2" != "1" ] && { echo "   ✗ Doc2 published wrong: $published2"; errors=$((errors+1)); }
    [ "$len3" != "2" ] && { echo "   ✗ Doc3 list length wrong: $len3"; errors=$((errors+1)); }
    [ "$item1" != "first" ] && { echo "   ✗ Doc3 item[0] wrong: $item1"; errors=$((errors+1)); }
    [ "$item2" != "second" ] && { echo "   ✗ Doc3 item[1] wrong: $item2"; errors=$((errors+1)); }

    if [ $errors -eq 0 ]; then
        echo "   ✓ Multiple documents survive Redis restart"
    else
        exit 1
    fi
else
    echo "   ⚠ Skipped restart test (Docker not available)"
fi

echo "Test 3: Nested paths persist after restart..."
redis-cli -h "$HOST" del persist_nested > /dev/null
redis-cli -h "$HOST" am.new persist_nested > /dev/null
redis-cli -h "$HOST" am.puttext persist_nested user.profile.name "Alice" > /dev/null
redis-cli -h "$HOST" am.putint persist_nested user.profile.age 28 > /dev/null
redis-cli -h "$HOST" am.puttext persist_nested user.settings.theme "dark" > /dev/null
redis-cli -h "$HOST" am.putbool persist_nested user.active true > /dev/null

# Try to restart if in Docker environment
if restart_redis; then
    # Verify nested data after restart
    name=$(redis-cli -h "$HOST" --raw am.gettext persist_nested user.profile.name)
    age=$(redis-cli -h "$HOST" am.getint persist_nested user.profile.age)
    theme=$(redis-cli -h "$HOST" --raw am.gettext persist_nested user.settings.theme)
    active=$(redis-cli -h "$HOST" am.getbool persist_nested user.active)

    errors=0
    [ "$name" != "Alice" ] && { echo "   ✗ Nested name wrong: $name"; errors=$((errors+1)); }
    [ "$age" != "28" ] && { echo "   ✗ Nested age wrong: $age"; errors=$((errors+1)); }
    [ "$theme" != "dark" ] && { echo "   ✗ Nested theme wrong: $theme"; errors=$((errors+1)); }
    [ "$active" != "1" ] && { echo "   ✗ Nested active wrong: $active"; errors=$((errors+1)); }

    if [ $errors -eq 0 ]; then
        echo "   ✓ Nested paths survive Redis restart"
    else
        exit 1
    fi
else
    echo "   ⚠ Skipped restart test (Docker not available)"
fi

echo "Test 4: Persistence after AOF rewrite..."
redis-cli -h "$HOST" del aof_rewrite_test > /dev/null
redis-cli -h "$HOST" am.new aof_rewrite_test > /dev/null
redis-cli -h "$HOST" am.puttext aof_rewrite_test data "Before rewrite" > /dev/null
redis-cli -h "$HOST" am.putint aof_rewrite_test counter 42 > /dev/null

# Force AOF rewrite
redis-cli -h "$HOST" BGREWRITEAOF > /dev/null
sleep 2  # Wait for rewrite to complete

# Modify data after rewrite
redis-cli -h "$HOST" am.puttext aof_rewrite_test data "After rewrite" > /dev/null
redis-cli -h "$HOST" am.putint aof_rewrite_test counter 100 > /dev/null

# Try to restart if in Docker environment
if restart_redis; then
    # Verify data after restart (should have post-rewrite values)
    data=$(redis-cli -h "$HOST" --raw am.gettext aof_rewrite_test data)
    counter=$(redis-cli -h "$HOST" am.getint aof_rewrite_test counter)

    if [ "$data" = "After rewrite" ] && [ "$counter" = "100" ]; then
        echo "   ✓ Documents survive AOF rewrite and restart"
    else
        echo "   ✗ Data corrupted after AOF rewrite (data='$data', counter='$counter')"
        exit 1
    fi
else
    echo "   ⚠ Skipped restart test (Docker not available)"
fi

echo "Test 5: Comprehensive persistence scenario..."
redis-cli -h "$HOST" del comprehensive_persist > /dev/null
redis-cli -h "$HOST" am.new comprehensive_persist > /dev/null

# Add various types of data
redis-cli -h "$HOST" am.puttext comprehensive_persist greeting "Hello World" > /dev/null
redis-cli -h "$HOST" am.putint comprehensive_persist count 42 > /dev/null
redis-cli -h "$HOST" am.putdouble comprehensive_persist pi 3.14159 > /dev/null
redis-cli -h "$HOST" am.putbool comprehensive_persist active true > /dev/null
redis-cli -h "$HOST" am.putcounter comprehensive_persist views 1000 > /dev/null
redis-cli -h "$HOST" am.inccounter comprehensive_persist views 500 > /dev/null

# Add nested data
redis-cli -h "$HOST" am.puttext comprehensive_persist user.name "Bob" > /dev/null
redis-cli -h "$HOST" am.putint comprehensive_persist user.age 30 > /dev/null

# Add lists
redis-cli -h "$HOST" am.createlist comprehensive_persist tags > /dev/null
redis-cli -h "$HOST" am.appendtext comprehensive_persist tags "redis" > /dev/null
redis-cli -h "$HOST" am.appendtext comprehensive_persist tags "crdt" > /dev/null
redis-cli -h "$HOST" am.appendtext comprehensive_persist tags "automerge" > /dev/null

# Force AOF rewrite
redis-cli -h "$HOST" BGREWRITEAOF > /dev/null
sleep 2

# Try to restart if in Docker environment
if restart_redis; then
    # Verify all data types after restart
    errors=0

    val=$(redis-cli -h "$HOST" --raw am.gettext comprehensive_persist greeting)
    [ "$val" != "Hello World" ] && { echo "   ✗ Text wrong: $val"; errors=$((errors+1)); }

    val=$(redis-cli -h "$HOST" am.getint comprehensive_persist count)
    [ "$val" != "42" ] && { echo "   ✗ Int wrong: $val"; errors=$((errors+1)); }

    val=$(redis-cli -h "$HOST" am.getdouble comprehensive_persist pi)
    [ "$val" != "3.14159" ] && { echo "   ✗ Double wrong: $val"; errors=$((errors+1)); }

    val=$(redis-cli -h "$HOST" am.getbool comprehensive_persist active)
    [ "$val" != "1" ] && { echo "   ✗ Bool wrong: $val"; errors=$((errors+1)); }

    val=$(redis-cli -h "$HOST" am.getcounter comprehensive_persist views)
    [ "$val" != "1500" ] && { echo "   ✗ Counter wrong: $val"; errors=$((errors+1)); }

    val=$(redis-cli -h "$HOST" --raw am.gettext comprehensive_persist user.name)
    [ "$val" != "Bob" ] && { echo "   ✗ Nested text wrong: $val"; errors=$((errors+1)); }

    val=$(redis-cli -h "$HOST" am.getint comprehensive_persist user.age)
    [ "$val" != "30" ] && { echo "   ✗ Nested int wrong: $val"; errors=$((errors+1)); }

    val=$(redis-cli -h "$HOST" am.listlen comprehensive_persist tags)
    [ "$val" != "3" ] && { echo "   ✗ List length wrong: $val"; errors=$((errors+1)); }

    val=$(redis-cli -h "$HOST" --raw am.gettext comprehensive_persist 'tags[0]')
    [ "$val" != "redis" ] && { echo "   ✗ List item[0] wrong: $val"; errors=$((errors+1)); }

    val=$(redis-cli -h "$HOST" --raw am.gettext comprehensive_persist 'tags[2]')
    [ "$val" != "automerge" ] && { echo "   ✗ List item[2] wrong: $val"; errors=$((errors+1)); }

    if [ $errors -eq 0 ]; then
        echo "   ✓ All data types survive comprehensive persistence test"
    else
        exit 1
    fi
else
    echo "   ⚠ Skipped restart test (Docker not available)"
fi

echo ""
echo "✅ All AOF persistence tests passed!"
