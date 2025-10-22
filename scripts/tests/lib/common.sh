#!/usr/bin/env bash
# Common test utilities for Redis Automerge Module tests

# Redis host configuration
HOST="${REDIS_HOST:-127.0.0.1}"

# Color output for better readability
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

# Assert function with helpful error messages
assert_equals() {
    local actual="$1"
    local expected="$2"
    local description="${3:-}"

    if [ "$actual" != "$expected" ]; then
        echo ""
        echo "❌ ASSERTION FAILED"
        if [ -n "$description" ]; then
            echo "   Description: $description"
        fi
        echo "   Expected: '$expected'"
        echo "   Actual:   '$actual'"
        echo ""
        exit 1
    fi
}

# Test keyspace notifications
test_notification() {
    local key=$1
    local expected_event=$2
    shift 2
    local command="$@"

    local output_file="/tmp/notif_test_$$.txt"

    # Start subscriber in background with timeout (1 second)
    timeout 1 redis-cli -h "$HOST" PSUBSCRIBE "__keyspace@0__:$key" > "$output_file" 2>&1 &
    local sub_pid=$!

    # Wait for subscription to be ready
    sleep 0.3

    # Run the command
    eval "$command" > /dev/null 2>&1

    # Wait for notification and subscriber to timeout
    wait $sub_pid 2>/dev/null || true

    # Check output
    if grep -q "$expected_event" "$output_file"; then
        rm -f "$output_file"
        return 0
    else
        echo "   ✗ Expected notification '$expected_event' not found"
        echo "   Output was:"
        cat "$output_file"
        rm -f "$output_file"
        return 1
    fi
}

# Test change publication to changes:key channel
test_change_publication() {
    local key=$1
    shift
    local command="$@"

    local output_file="/tmp/change_pub_$$.txt"

    # Subscribe to changes channel
    timeout 2 redis-cli -h "$HOST" SUBSCRIBE "changes:$key" > "$output_file" 2>&1 &
    local sub_pid=$!
    sleep 0.3

    # Execute command
    eval "$command" > /dev/null 2>&1
    sleep 0.3

    # Kill subscriber
    kill $sub_pid 2>/dev/null || true
    wait $sub_pid 2>/dev/null || true

    # Verify change was published
    if [ -f "$output_file" ] && grep -q "changes:$key" "$output_file"; then
        rm -f "$output_file"
        return 0
    else
        echo "   ✗ Expected change publication not found"
        [ -f "$output_file" ] && cat "$output_file"
        rm -f "$output_file"
        return 1
    fi
}

# Setup function - ensure server is up
setup_test_env() {
    echo "Setting up test environment..."

    # Install jq for JSON validation if not already installed
    if ! command -v jq &> /dev/null; then
        echo "Installing jq..."
        apt-get update -qq > /dev/null 2>&1 && apt-get install -y -qq jq > /dev/null 2>&1
    fi

    # Check server connection
    if ! redis-cli -h "$HOST" ping > /dev/null 2>&1; then
        echo "❌ Cannot connect to Redis at $HOST"
        exit 1
    fi

    # Clear any persisted data
    redis-cli -h "$HOST" flushall > /dev/null 2>&1

    echo "✓ Test environment ready"
}

# Print section header
print_section() {
    local title="$1"
    echo ""
    echo "========================================="
    echo "$title"
    echo "========================================="
}

# Helper function to restart Redis
# Uses DEBUG RESTART which simulates a server restart by reloading from AOF/RDB
restart_redis() {
    echo "   Restarting Redis (DEBUG RESTART)..."
    # DEBUG RESTART will cause Redis to reload from AOF/RDB
    # It will disconnect us, so we need to handle the error
    redis-cli -h "$HOST" DEBUG RESTART > /dev/null 2>&1 || true
    sleep 2

    # Wait for Redis to be ready
    for i in {1..15}; do
        if redis-cli -h "$HOST" ping > /dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    echo "   ✗ Redis failed to restart"
    return 1
}
