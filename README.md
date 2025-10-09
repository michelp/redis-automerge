# Redis-Automerge

[![CI](https://github.com/<username>/redis-automerge/actions/workflows/ci.yml/badge.svg)](https://github.com/<username>/redis-automerge/actions/workflows/ci.yml)
[![Documentation](https://github.com/<username>/redis-automerge/actions/workflows/docs.yml/badge.svg)](https://github.com/<username>/redis-automerge/actions/workflows/docs.yml)

A Redis module that integrates [Automerge](https://automerge.org/) CRDT (Conflict-free Replicated Data Type) documents into Redis, providing JSON-like document storage with automatic conflict resolution.

## Features

- **JSON-like document storage** with RedisJSON-compatible path syntax
- **Automatic conflict resolution** using Automerge CRDTs
- **Nested data structures** - maps and arrays with dot notation and array indices
- **Type-safe operations** - text, integers, doubles, and booleans
- **Persistent storage** via Redis RDB and AOF
- **Replication support** for Redis clusters

## Building

### Requirements

- Rust 1.70+ with Cargo
- Docker (for integration tests)
- Clang (for building)

### Build from Source

```bash
cargo build --release --manifest-path redis-automerge/Cargo.toml
```

The compiled module will be at `redis-automerge/target/release/libredis_automerge.so`

### Build with Docker

```bash
docker compose build
```

## Running

### Load Module in Redis

```bash
redis-server --loadmodule /path/to/libredis_automerge.so
```

### Using Docker Compose

```bash
docker compose up redis
```

## Redis Commands

### Document Management

#### `AM.NEW <key>`
Create a new empty Automerge document.

```redis
AM.NEW mydoc
```

#### `AM.SAVE <key>`
Save a document to binary format (for backup or transfer).

```redis
AM.SAVE mydoc
```

#### `AM.LOAD <key> <bytes>`
Load a document from binary format.

```redis
AM.LOAD mydoc <binary-data>
```

#### `AM.APPLY <key> <change>...`
Apply Automerge changes to a document (for synchronization).

```redis
AM.APPLY mydoc <change1> <change2>
```

### Value Operations

#### `AM.PUTTEXT <key> <path> <value>`
Set a text value at the specified path.

```redis
AM.PUTTEXT mydoc user.name "Alice"
AM.PUTTEXT mydoc $.config.host "localhost"
```

#### `AM.GETTEXT <key> <path>`
Get a text value from the specified path.

```redis
AM.GETTEXT mydoc user.name
# Returns: "Alice"
```

#### `AM.PUTINT <key> <path> <value>`
Set an integer value.

```redis
AM.PUTINT mydoc user.age 30
AM.PUTINT mydoc config.port 6379
```

#### `AM.GETINT <key> <path>`
Get an integer value.

```redis
AM.GETINT mydoc user.age
# Returns: 30
```

#### `AM.PUTDOUBLE <key> <path> <value>`
Set a double/float value.

```redis
AM.PUTDOUBLE mydoc metrics.cpu 75.5
AM.PUTDOUBLE mydoc data.temperature 98.6
```

#### `AM.GETDOUBLE <key> <path>`
Get a double value.

```redis
AM.GETDOUBLE mydoc metrics.cpu
# Returns: 75.5
```

#### `AM.PUTBOOL <key> <path> <value>`
Set a boolean value (accepts: true/false, 1/0).

```redis
AM.PUTBOOL mydoc user.active true
AM.PUTBOOL mydoc flags.debug 0
```

#### `AM.GETBOOL <key> <path>`
Get a boolean value (returns 1 for true, 0 for false).

```redis
AM.GETBOOL mydoc user.active
# Returns: 1
```

### List Operations

#### `AM.CREATELIST <key> <path>`
Create a new empty list at the specified path.

```redis
AM.CREATELIST mydoc users
AM.CREATELIST mydoc data.items
```

#### `AM.APPENDTEXT <key> <path> <value>`
Append a text value to a list.

```redis
AM.APPENDTEXT mydoc users "Alice"
AM.APPENDTEXT mydoc users "Bob"
```

#### `AM.APPENDINT <key> <path> <value>`
Append an integer to a list.

```redis
AM.APPENDINT mydoc scores 100
AM.APPENDINT mydoc scores 95
```

#### `AM.APPENDDOUBLE <key> <path> <value>`
Append a double to a list.

```redis
AM.APPENDDOUBLE mydoc temperatures 98.6
AM.APPENDDOUBLE mydoc temperatures 99.1
```

#### `AM.APPENDBOOL <key> <path> <value>`
Append a boolean to a list.

```redis
AM.APPENDBOOL mydoc flags true
AM.APPENDBOOL mydoc flags false
```

#### `AM.LISTLEN <key> <path>`
Get the length of a list.

```redis
AM.LISTLEN mydoc users
# Returns: 2
```

## Path Syntax

The module supports RedisJSON-compatible path syntax:

### Simple Keys
```redis
AM.PUTTEXT mydoc name "Alice"
AM.PUTINT mydoc age 30
```

### Nested Maps (Dot Notation)
```redis
AM.PUTTEXT mydoc user.profile.name "Alice"
AM.PUTINT mydoc config.database.port 5432
```

### Array Indices
```redis
AM.CREATELIST mydoc users
AM.APPENDTEXT mydoc users "Alice"
AM.GETTEXT mydoc users[0]
# Returns: "Alice"
```

### Mixed Paths
```redis
AM.CREATELIST mydoc data.items
AM.APPENDTEXT mydoc data.items "first"
AM.GETTEXT mydoc data.items[0]
# Returns: "first"
```

### JSONPath Style (with $ prefix)
```redis
AM.PUTTEXT mydoc $.user.name "Alice"
AM.GETTEXT mydoc $.users[0].profile.name
```

## Examples

### User Profile

```redis
# Create document
AM.NEW user:1001

# Set user data
AM.PUTTEXT user:1001 name "Alice Smith"
AM.PUTINT user:1001 age 28
AM.PUTTEXT user:1001 email "alice@example.com"
AM.PUTBOOL user:1001 verified true

# Create nested profile
AM.PUTTEXT user:1001 profile.bio "Software Engineer"
AM.PUTTEXT user:1001 profile.location "San Francisco"

# Get values
AM.GETTEXT user:1001 name
# Returns: "Alice Smith"

AM.GETTEXT user:1001 profile.location
# Returns: "San Francisco"
```

### Shopping Cart with Items

```redis
# Create document
AM.NEW cart:5001

# Add cart metadata
AM.PUTTEXT cart:5001 user_id "user:1001"
AM.PUTINT cart:5001 total 0

# Create items list
AM.CREATELIST cart:5001 items

# Add first item (as text for simplicity)
AM.APPENDTEXT cart:5001 items "Product A"
AM.APPENDTEXT cart:5001 items "Product B"
AM.APPENDTEXT cart:5001 items "Product C"

# Get item count
AM.LISTLEN cart:5001 items
# Returns: 3

# Get specific item
AM.GETTEXT cart:5001 items[1]
# Returns: "Product B"
```

### Configuration Document

```redis
# Create config
AM.NEW config:main

# Database settings
AM.PUTTEXT config:main database.host "localhost"
AM.PUTINT config:main database.port 5432
AM.PUTTEXT config:main database.name "myapp"

# Cache settings
AM.PUTTEXT config:main cache.host "localhost"
AM.PUTINT config:main cache.port 6379
AM.PUTBOOL config:main cache.enabled true

# Feature flags list
AM.CREATELIST config:main features
AM.APPENDTEXT config:main features "new-ui"
AM.APPENDTEXT config:main features "api-v2"
AM.APPENDTEXT config:main features "analytics"

# Get configuration
AM.GETTEXT config:main database.host
# Returns: "localhost"

AM.GETBOOL config:main cache.enabled
# Returns: 1

AM.LISTLEN config:main features
# Returns: 3
```

## Testing

### Unit Tests

```bash
cargo test --verbose --manifest-path redis-automerge/Cargo.toml
```

### Integration Tests

```bash
# Run integration tests with Docker
docker compose run --build --rm test
docker compose down
```

### Full Test Suite

```bash
# Run both unit and integration tests
cargo test --verbose --manifest-path redis-automerge/Cargo.toml
docker compose run --build --rm test
docker compose down
```

## Documentation

### Online Documentation

API documentation is automatically built and deployed to GitHub Pages:
- **Latest docs**: `https://<username>.github.io/redis-automerge/`

Documentation is updated automatically on every push to main.

### Generate Locally

```bash
cargo doc --no-deps --manifest-path redis-automerge/Cargo.toml --open
```

This generates detailed API documentation for the Rust code and opens it in your browser.

## Architecture

- **`redis-automerge/src/lib.rs`** - Redis module interface, command handlers, RDB/AOF persistence
- **`redis-automerge/src/ext.rs`** - Automerge integration layer, path parsing, CRDT operations

### Key Components

1. **Path Parser** - Converts RedisJSON-style paths to internal segments
2. **Navigation** - Traverses nested maps and lists, creates intermediate structures
3. **Type Operations** - Type-safe get/put operations for different data types
4. **List Operations** - Create lists, append values, get length
5. **Persistence** - RDB save/load and AOF change tracking
6. **Replication** - Change propagation to Redis replicas

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](.github/CONTRIBUTING.md) for detailed guidelines.

### Quick Start

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes with tests
4. Ensure all checks pass:
   ```bash
   cargo test --manifest-path redis-automerge/Cargo.toml
   cargo fmt --manifest-path redis-automerge/Cargo.toml
   cargo clippy --manifest-path redis-automerge/Cargo.toml
   docker compose run --build --rm test
   ```
5. Commit and push (`git push origin feature/amazing-feature`)
6. Open a Pull Request

All PRs are automatically tested via GitHub Actions.

## Resources

- [Automerge Documentation](https://automerge.org/)
- [Redis Module API](https://redis.io/topics/modules-intro)
- [RedisJSON](https://redis.io/docs/stack/json/) - Similar path syntax reference
