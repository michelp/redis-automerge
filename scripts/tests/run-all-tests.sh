#!/usr/bin/env bash
# Main test runner for Redis Automerge Module
# Executes all test suites in order

set -uo pipefail

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Load common utilities for setup
source "$SCRIPT_DIR/lib/common.sh"

# Track results
TESTS_PASSED=0
TESTS_FAILED=0
FAILED_SUITES=()

# Run setup once
setup_test_env

echo ""
echo "========================================="
echo "Redis Automerge Module Test Suite"
echo "========================================="
echo ""

# Find all test files (XX-*.sh pattern)
TEST_FILES=$(find "$SCRIPT_DIR" -maxdepth 1 -name "[0-9][0-9]-*.sh" | sort)

if [ -z "$TEST_FILES" ]; then
    echo -e "${RED}❌ No test files found!${NC}"
    exit 1
fi

# Run each test suite
for test_file in $TEST_FILES; do
    test_name=$(basename "$test_file" .sh)

    echo ""
    echo -e "${BLUE}Running: $test_name${NC}"
    echo "========================================="

    if bash "$test_file"; then
        echo -e "${GREEN}✅ $test_name PASSED${NC}"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}❌ $test_name FAILED${NC}"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        FAILED_SUITES+=("$test_name")
    fi
done

# Print summary
echo ""
echo "========================================="
echo "Test Summary"
echo "========================================="
echo "Total suites: $((TESTS_PASSED + TESTS_FAILED))"
echo -e "${GREEN}Passed: $TESTS_PASSED${NC}"
if [ $TESTS_FAILED -gt 0 ]; then
    echo -e "${RED}Failed: $TESTS_FAILED${NC}"
    echo ""
    echo "Failed suites:"
    for suite in "${FAILED_SUITES[@]}"; do
        echo "  - $suite"
    done
    exit 1
else
    echo -e "${GREEN}All test suites passed!${NC}"
    exit 0
fi
