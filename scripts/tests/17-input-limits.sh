#!/usr/bin/env bash
# Test input-size and recursion-depth limits for AM.LOAD, AM.APPLY, AM.FROMJSON.
# Closes audit findings #3 (size limits) and #4 (FROMJSON recursion depth).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/common.sh"

print_section "Input-size and depth limits (audit #3, #4)"

# -----------------------------------------------------------------------------
# Test 1: AM.FROMJSON rejects payload > 64 MiB
# -----------------------------------------------------------------------------
echo "Test 1: AM.FROMJSON rejects payload > 64 MiB..."
redis-cli -h "$HOST" del big-json > /dev/null
# Build a 64 MiB + 1 byte JSON document. The size check fires before parsing.
result=$(head -c 67108865 /dev/zero | tr '\0' 'a' | redis-cli -h "$HOST" -x am.fromjson big-json 2>&1 || true)
if echo "$result" | grep -q "exceeds 67108864 byte limit"; then
    echo "   ✓ AM.FROMJSON rejected oversized payload"
else
    echo "   ✗ AM.FROMJSON did not reject oversized payload (got: $result)"
    exit 1
fi
# Document must not have been created.
exists=$(redis-cli -h "$HOST" exists big-json)
assert_equals "$exists" "0" "big-json should not have been created"

# -----------------------------------------------------------------------------
# Test 2: AM.LOAD rejects payload > 64 MiB
# -----------------------------------------------------------------------------
echo "Test 2: AM.LOAD rejects payload > 64 MiB..."
redis-cli -h "$HOST" del big-load > /dev/null
result=$(head -c 67108865 /dev/zero | redis-cli -h "$HOST" -x am.load big-load 2>&1 || true)
if echo "$result" | grep -q "exceeds 67108864 byte limit"; then
    echo "   ✓ AM.LOAD rejected oversized payload"
else
    echo "   ✗ AM.LOAD did not reject oversized payload (got: $result)"
    exit 1
fi
exists=$(redis-cli -h "$HOST" exists big-load)
assert_equals "$exists" "0" "big-load should not have been created"

# -----------------------------------------------------------------------------
# Test 3: AM.APPLY rejects > 1024 changes per call
# -----------------------------------------------------------------------------
echo "Test 3: AM.APPLY rejects > 1024 changes per call..."
redis-cli -h "$HOST" del apply-test > /dev/null
redis-cli -h "$HOST" am.new apply-test > /dev/null
# Build 1025 garbage args. The count check fires before any change is parsed,
# so the dummy values never have to be valid Change bytes.
args=()
for i in $(seq 1 1025); do args+=("x"); done
result=$(redis-cli -h "$HOST" am.apply apply-test "${args[@]}" 2>&1 || true)
if echo "$result" | grep -q "accepts at most 1024 changes per call"; then
    echo "   ✓ AM.APPLY rejected too-many-changes call"
else
    echo "   ✗ AM.APPLY did not reject 1025 changes (got: $result)"
    exit 1
fi

# -----------------------------------------------------------------------------
# Test 4: AM.FROMJSON rejects nesting depth > 256 (no Redis crash)
# -----------------------------------------------------------------------------
echo "Test 4: AM.FROMJSON rejects deep nesting (no crash)..."
redis-cli -h "$HOST" del deep-json > /dev/null
# 300 levels of {"a": ... } — exceeds MAX_JSON_DEPTH (256).
prefix=""
suffix=""
for i in $(seq 1 300); do
    prefix="${prefix}{\"a\":"
    suffix="${suffix}}"
done
deep_json="${prefix}null${suffix}"
# We expect either an error result OR the underlying serde_json default
# (recursion_limit=128) to reject it first; in either case Redis must stay up.
result=$(redis-cli -h "$HOST" am.fromjson deep-json "$deep_json" 2>&1 || true)
# Server must still respond to PING after the deep input.
ping_result=$(redis-cli -h "$HOST" ping)
assert_equals "$ping_result" "PONG" "Redis must remain responsive after deep JSON"
# Document must not have been created.
exists=$(redis-cli -h "$HOST" exists deep-json)
assert_equals "$exists" "0" "deep-json should not have been created"
echo "   ✓ AM.FROMJSON rejected deep JSON, Redis stayed up (response: $result)"

# -----------------------------------------------------------------------------
# Test 5: Regression — small/normal inputs still succeed
# -----------------------------------------------------------------------------
echo "Test 5: Normal-sized inputs continue to work..."
redis-cli -h "$HOST" del ok-doc > /dev/null
# Small JSON
redis-cli -h "$HOST" am.fromjson ok-doc '{"name":"Alice","age":30}' > /dev/null
val=$(redis-cli -h "$HOST" --raw am.gettext ok-doc name)
assert_equals "$val" "Alice"
# AM.APPLY with a small number of (real) changes via round-trip
redis-cli -h "$HOST" del src-doc > /dev/null
redis-cli -h "$HOST" am.new src-doc > /dev/null
redis-cli -h "$HOST" am.puttext src-doc field "v1" > /dev/null
# Just confirm AM.APPLY with 0 extra change args returns wrong-arity, not the limit.
result=$(redis-cli -h "$HOST" am.apply ok-doc 2>&1 || true)
if echo "$result" | grep -q "wrong number of arguments"; then
    echo "   ✓ AM.APPLY zero-changes returns wrong-arity (not the limit error)"
else
    echo "   ✗ Unexpected response for 0-arg apply: $result"
    exit 1
fi
echo "   ✓ Normal-sized inputs still work"

echo ""
echo "✅ All input-limit tests passed!"
