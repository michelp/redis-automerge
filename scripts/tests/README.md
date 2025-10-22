# Redis Automerge Module - Test Suite

This directory contains the modular test suite for the Redis Automerge Module. Tests are organized into separate files by functionality for easier understanding and maintenance.

## Structure

```
tests/
├── lib/
│   └── common.sh           # Shared test utilities and functions
├── 01-basic-types.sh       # Basic type operations (text, int, double, bool, counter)
├── 02-nested-paths.sh      # Nested path and JSONPath operations
├── 03-lists.sh             # List creation and operations
├── 04-notifications.sh     # Keyspace notification events
├── 05-text-operations.sh   # AM.PUTDIFF and AM.SPLICETEXT
├── 06-change-publishing.sh # Pub/sub change publication
├── 07-change-management.sh # AM.CHANGES, AM.NUMCHANGES, AM.APPLY
├── 08-json-operations.sh   # AM.TOJSON and AM.FROMJSON
├── 09-timestamps.sh        # Timestamp operations
├── 10-aof-persistence.sh   # AOF persistence and restart
├── run-all-tests.sh        # Main test runner
└── README.md               # This file
```

## Running Tests

### Run All Tests

```bash
# From the repository root
docker compose run --build --rm test

# Or directly
./scripts/tests/run-all-tests.sh
```

### Run Individual Test Suite

```bash
# Run just basic type tests
./scripts/tests/01-basic-types.sh

# Run just JSON operations
./scripts/tests/08-json-operations.sh
```

### Environment Variables

- `REDIS_HOST` - Redis server hostname (default: 127.0.0.1)

```bash
REDIS_HOST=redis ./scripts/tests/run-all-tests.sh
```

## Test Categories

### 01-basic-types.sh
Tests for fundamental data types:
- Text (string) operations
- Integer operations (positive and negative)
- Double (floating point) operations
- Boolean operations
- Counter operations (CRDT counter type)
- Mixed types in single document
- Persistence of all types

### 02-nested-paths.sh
Tests for nested path operations:
- Nested map keys (e.g., `user.profile.name`)
- JSONPath-style syntax with `$` prefix
- Deeply nested structures
- Mixed flat and nested keys
- Persistence of nested paths

### 03-lists.sh
Tests for list/array operations:
- List creation with `AM.CREATELIST`
- Appending different types to lists
- Index-based access (e.g., `items[0]`)
- Mixed types within lists
- Nested list paths
- List persistence

### 04-notifications.sh
Tests for Redis keyspace notifications:
- Notification events for all write commands
- Proper event naming
- Subscriber reception

### 05-text-operations.sh
Tests for advanced text operations:
- `AM.PUTDIFF` - Apply unified diffs to text
- `AM.SPLICETEXT` - In-place text editing
- Text object conversion
- Persistence of text operations

### 06-change-publishing.sh
Tests for automatic change publication:
- Changes published to `changes:{key}` channels
- Binary change data delivery
- Subscriber reception
- Change publication for all write operations

### 07-change-management.sh
Tests for change tracking and synchronization:
- `AM.CHANGES` - Retrieve change bytes
- `AM.NUMCHANGES` - Count changes
- `AM.APPLY` - Apply changes to documents
- Change persistence across save/load
- Document synchronization patterns

### 08-json-operations.sh
Tests for JSON import/export:
- `AM.TOJSON` - Export to JSON
- `AM.FROMJSON` - Import from JSON
- Compact vs pretty formatting
- Complex nested structures
- Roundtrip conversion
- Timestamp ISO 8601 formatting

### 09-timestamps.sh
Tests for timestamp operations:
- `AM.PUTTIMESTAMP` / `AM.GETTIMESTAMP`
- Unix millisecond format
- Nested timestamp paths
- JSON export as ISO 8601
- Persistence

### 10-aof-persistence.sh
Tests for AOF (Append-Only File) persistence:
- Persistence after server restart
- Multiple documents
- Nested paths
- AOF rewrite operations
- Comprehensive persistence scenarios

## Common Test Library

The `lib/common.sh` file provides shared utilities:

### Functions

- `assert_equals(actual, expected, [description])` - Assert equality with helpful error messages
- `test_notification(key, event, command)` - Test keyspace notifications
- `test_change_publication(key, command)` - Test change publication to pub/sub
- `setup_test_env()` - Initialize test environment
- `print_section(title)` - Print formatted section header

### Variables

- `$HOST` - Redis server hostname (from `REDIS_HOST` env var)

## Writing New Tests

1. Create a new file with naming pattern `NN-category.sh` (e.g., `11-my-feature.sh`)
2. Add standard header:
   ```bash
   #!/usr/bin/env bash
   set -euo pipefail

   SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
   source "$SCRIPT_DIR/lib/common.sh"

   print_section "My Feature Tests"
   ```
3. Write tests using `assert_equals()` and other utilities
4. Make executable: `chmod +x scripts/tests/11-my-feature.sh`
5. Run with `run-all-tests.sh` (auto-discovered) or individually

## Migration from Monolithic Script

The original `test-module.sh` has been split into these modular files. To complete the migration:

1. Extract remaining tests from `test-module.sh` into appropriate category files
2. Ensure all 94 original tests are covered
3. Verify all tests pass with `run-all-tests.sh`
4. Update Docker and CI configurations to use new test structure
5. Archive or remove old `test-module.sh`

## Benefits of Modular Structure

- **Easier to understand** - Tests grouped by functionality
- **Faster iteration** - Run only relevant test suites
- **Better maintenance** - Changes isolated to specific files
- **Clearer failures** - Immediately see which category failed
- **Parallel execution** - Can run suites concurrently if needed
- **Better documentation** - Each file documents its category
