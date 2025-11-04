# Redis-Automerge

[![CI](https://github.com/michelp/redis-automerge/actions/workflows/ci.yml/badge.svg)](https://github.com/michelp/redis-automerge/actions/workflows/ci.yml)
[![Documentation](https://github.com/michelp/redis-automerge/actions/workflows/docs.yml/badge.svg)](https://github.com/michelp/redis-automerge/actions/workflows/docs.yml)
[![Docker Hub](https://img.shields.io/docker/v/metagration/redis-automerge?label=docker&logo=docker)](https://hub.docker.com/r/metagration/redis-automerge)
[![Docker Pulls](https://img.shields.io/docker/pulls/metagration/redis-automerge)](https://hub.docker.com/r/metagration/redis-automerge)

A Redis module that integrates [Automerge](https://automerge.org/) CRDT (Conflict-free Replicated Data Type) documents into Redis, providing JSON-like document storage with automatic conflict resolution.

## Table of Contents

- [Features](#features)
- [Quick Start with Docker](#quick-start-with-docker)
  - [Pre-Built Images from Docker Hub](#pre-built-images-from-docker-hub)
    - [Pull Latest Image](#pull-latest-image)
    - [Pull Specific Version](#pull-specific-version)
  - [Using Docker Compose](#using-docker-compose)
  - [Available Tags](#available-tags)
  - [Updating to Latest Version](#updating-to-latest-version)
- [Building](#building)
  - [Requirements](#requirements)
  - [Build from Source](#build-from-source)
  - [Build with Docker](#build-with-docker)
- [Running](#running)
  - [Load Module in Redis](#load-module-in-redis)
  - [Using Docker Compose](#using-docker-compose-1)
- [Redis Commands](#redis-commands)
  - [Document Management](#document-management)
    - [`AM.NEW <key>`](#amnew-key)
    - [`AM.SAVE <key>`](#amsave-key)
    - [`AM.LOAD <key> <bytes>`](#amload-key-bytes)
    - [`AM.APPLY <key> <change>...`](#amapply-key-change)
    - [`AM.CHANGES <key> [<hash>...]`](#amchanges-key-hash)
    - [`AM.GETDIFF <key> BEFORE <hash>... AFTER <hash>...`](#amgetdiff-key-before-hash-after-hash)
    - [`AM.TOJSON <key> [pretty]`](#amtojson-key-pretty)
    - [`AM.FROMJSON <key> <json>`](#amfromjson-key-json)
  - [Value Operations](#value-operations)
    - [`AM.PUTTEXT <key> <path> <value>`](#amputtext-key-path-value)
    - [`AM.GETTEXT <key> <path>`](#amgettext-key-path)
    - [`AM.SPLICETEXT <key> <path> <pos> <del> <text>`](#amsplicetext-key-path-pos-del-text)
    - [`AM.PUTDIFF <key> <path> <diff>`](#amputdiff-key-path-diff)
    - [`AM.PUTINT <key> <path> <value>`](#amputint-key-path-value)
    - [`AM.GETINT <key> <path>`](#amgetint-key-path)
    - [`AM.PUTDOUBLE <key> <path> <value>`](#amputdouble-key-path-value)
    - [`AM.GETDOUBLE <key> <path>`](#amgetdouble-key-path)
    - [`AM.PUTBOOL <key> <path> <value>`](#amputbool-key-path-value)
    - [`AM.GETBOOL <key> <path>`](#amgetbool-key-path)
    - [`AM.PUTCOUNTER <key> <path> <value>`](#amputcounter-key-path-value)
    - [`AM.GETCOUNTER <key> <path>`](#amgetcounter-key-path)
    - [`AM.INCCOUNTER <key> <path> <delta>`](#aminccounter-key-path-delta)
  - [Text Marks Operations](#text-marks-operations)
    - [`AM.MARKCREATE <key> <path> <name> <value> <start> <end> [expand]`](#ammarkcreate-key-path-name-value-start-end-expand)
    - [`AM.MARKS <key> <path>`](#ammarks-key-path)
    - [`AM.MARKCLEAR <key> <path> <name> <start> <end> [expand]`](#ammarkclear-key-path-name-start-end-expand)
  - [List Operations](#list-operations)
    - [`AM.CREATELIST <key> <path>`](#amcreatelist-key-path)
    - [`AM.APPENDTEXT <key> <path> <value>`](#amappendtext-key-path-value)
    - [`AM.APPENDINT <key> <path> <value>`](#amappendint-key-path-value)
    - [`AM.APPENDDOUBLE <key> <path> <value>`](#amappenddouble-key-path-value)
    - [`AM.APPENDBOOL <key> <path> <value>`](#amappendbool-key-path-value)
    - [`AM.LISTLEN <key> <path>`](#amlistlen-key-path)
    - [`AM.MAPLEN <key> <path>`](#ammaplen-key-path)
- [Real-Time Synchronization](#real-time-synchronization)
  - [Change Notifications](#change-notifications)
  - [Subscribing to Changes](#subscribing-to-changes)
  - [Synchronization Pattern](#synchronization-pattern)
  - [Loading Document State](#loading-document-state)
- [Path Syntax](#path-syntax)
  - [Simple Keys](#simple-keys)
  - [Nested Maps (Dot Notation)](#nested-maps-dot-notation)
  - [Array Indices](#array-indices)
  - [Mixed Paths](#mixed-paths)
  - [JSONPath Style (with $ prefix)](#jsonpath-style-with--prefix)
- [Examples](#examples)
  - [User Profile](#user-profile)
  - [Shopping Cart with Items](#shopping-cart-with-items)
  - [Configuration Document](#configuration-document)
  - [Analytics with Counters](#analytics-with-counters)
  - [JSON Import/Export](#json-importexport)
  - [Rich Text Editor with Marks](#rich-text-editor-with-marks)
  - [Marks with Expand Behavior](#marks-with-expand-behavior)
  - [Collaborative Annotations](#collaborative-annotations)
- [Testing](#testing)
  - [Unit Tests](#unit-tests)
  - [Integration Tests](#integration-tests)
  - [Full Test Suite](#full-test-suite)
- [Documentation](#documentation)
  - [Online Documentation](#online-documentation)
  - [Generate Locally](#generate-locally)
- [Architecture](#architecture)
  - [Key Components](#key-components)
  - [Synchronization Flow](#synchronization-flow)
- [Resources](#resources)

## Features

- **JSON-like document storage** with RedisJSON-like path syntax
- **JSON import/export** - seamlessly convert between Automerge and JSON formats
- **Automatic conflict resolution** using Automerge CRDTs
- **Nested data structures** - maps and arrays with dot notation and array indices
- **Type-safe operations** - text, integers, doubles, booleans, and counters
- **CRDT counters** - distributed counters with proper conflict-free increment operations
- **Rich text marks** - annotate text ranges with formatting, links, comments, and custom metadata
- **Real-time synchronization** - pub/sub change notifications for live updates
- **Efficient text editing** - splice operations and unified diff support
- **Change history** - retrieve document changes for synchronization
- **Persistent storage** via Redis RDB and AOF
- **Replication support** for Redis clusters

## Quick Start with Docker

### Pre-Built Images from Docker Hub

The easiest way to get started is using pre-built images available on Docker Hub at **[metagration/redis-automerge](https://hub.docker.com/r/metagration/redis-automerge)**.

#### Pull Latest Image

```bash
# Pull the latest stable version
docker pull metagration/redis-automerge:latest

# Run Redis with the module loaded
docker run -d --name redis-automerge -p 6379:6379 metagration/redis-automerge:latest

# Test it works
redis-cli PING
redis-cli AM.NEW mydoc
redis-cli AM.PUTTEXT mydoc greeting "Hello, Automerge!"
redis-cli AM.GETTEXT mydoc greeting
# Returns: "Hello, Automerge!"
```

#### Pull Specific Version

```bash
# Pull a specific version
docker pull metagration/redis-automerge:1.0.0

# Run the specific version
docker run -d -p 6379:6379 metagration/redis-automerge:1.0.0
```

### Using Docker Compose

Create a `docker-compose.yml`:

```yaml
version: '3.8'
services:
  redis:
    image: metagration/redis-automerge:latest
    # Or pin to specific version for production:
    # image: metagration/redis-automerge:1.0.0
    ports:
      - "6379:6379"
    volumes:
      - redis-data:/data
    restart: unless-stopped

volumes:
  redis-data:
```

Then run:

```bash
# Start Redis with module
docker compose up -d

# View logs
docker compose logs -f redis

# Stop
docker compose down
```

### Available Tags

All images are automatically built and tested before publishing. When a version tag is pushed, the workflow automatically:
- Builds and tests the Docker image
- Pushes to Docker Hub (if tests pass)
- Creates a GitHub release with documentation

- **`latest`** - Latest stable release (recommended)
- **`1.0.0`, `1.0`, `1`** - Semantic version tags for specific releases

**Browse all tags**: https://hub.docker.com/r/metagration/redis-automerge/tags

**View releases**: https://github.com/michelp/redis-automerge/releases

### Updating to Latest Version

```bash
# Pull the latest image
docker pull metagration/redis-automerge:latest

# Recreate container with new image
docker compose down
docker compose up -d

# Or with plain docker
docker stop redis-automerge
docker rm redis-automerge
docker run -d --name redis-automerge -p 6379:6379 metagration/redis-automerge:latest
```

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
# Start Redis with module loaded
docker compose up redis

# Run integration tests
docker compose run --build --rm test
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
Apply one or more Automerge changes to a document. Used for synchronization between clients.

```redis
AM.APPLY mydoc <change1> <change2>
```

Each change is published to the `changes:{key}` Redis pub/sub channel as base64-encoded data, enabling real-time synchronization across all connected clients.

#### `AM.CHANGES <key> [<hash>...]`
Get changes from a document that are not in the provided dependency list. Returns all changes when no hashes are provided.

```redis
# Get all changes
AM.CHANGES mydoc

# Get only new changes (provide known change hashes)
AM.CHANGES mydoc <hash1> <hash2>
```

This command is essential for synchronizing document state between clients. A client can request only the changes it doesn't have by providing the hashes of changes it already knows about.

#### `AM.GETDIFF <key> BEFORE <hash>... AFTER <hash>...`
Get the diff between two document states. Returns a JSON array of patches describing what changed between the two states.

```redis
# Compare initial state (empty) to current state (empty arrays = all changes)
AM.GETDIFF mydoc BEFORE AFTER

# Compare two specific states by their change hashes
AM.GETDIFF mydoc BEFORE <hash1> AFTER <hash2> <hash3>
```

This command uses Automerge's diff functionality to compare two document states identified by their change hashes (heads). Each patch in the result describes a specific change including the path, type of operation, and values.

**Use cases:**
- Discovering what changed since a client's last sync
- Building change logs or audit trails
- Debugging document history
- Implementing incremental UI updates

**Example - Tracking document changes:**

```redis
# Create document and capture initial state
AM.NEW project
AM.PUTTEXT project name "Alpha"
AM.PUTINT project version 1

# Get current change hashes for "before" state
AM.CHANGES project
# Returns: [<hash1>, <hash2>, <hash3>]

# Make more changes
AM.PUTTEXT project name "Beta"
AM.PUTINT project version 2
AM.PUTTEXT project status "active"

# Get new change hashes for "after" state
AM.CHANGES project
# Returns: [<hash1>, <hash2>, <hash3>, <hash4>, <hash5>, <hash6>]

# Get diff showing what changed
AM.GETDIFF project BEFORE <hash1> <hash2> <hash3> AFTER <hash4> <hash5> <hash6>
# Returns JSON showing name changed from "Alpha" to "Beta",
# version changed from 1 to 2, and status was added
```

**Empty arrays:**
- Empty BEFORE (no hashes): represents initial/empty document state
- Empty AFTER (no hashes): represents current document state
- Both empty: shows diff from empty to current state

#### `AM.TOJSON <key> [pretty]`
Export an Automerge document to JSON format. Converts all maps, lists, and scalar values to their JSON equivalents.

```redis
# Export as compact JSON (default)
AM.TOJSON mydoc
# Returns: {"name":"Alice","age":30,"tags":["rust","redis"]}

# Export with pretty formatting (indented, multi-line)
AM.TOJSON mydoc true
# Returns:
# {
#   "name": "Alice",
#   "age": 30,
#   "tags": [
#     "rust",
#     "redis"
#   ]
# }
```

Parameters:
- `pretty` (optional) - Set to `true`, `1`, or `yes` for pretty-printed JSON. Defaults to compact format.

Type conversions:
- Automerge **Maps** → JSON objects `{}`
- Automerge **Lists** → JSON arrays `[]`
- Automerge **text** → JSON strings
- Automerge **integers** → JSON numbers
- Automerge **doubles** → JSON numbers
- Automerge **booleans** → JSON `true`/`false`
- Automerge **null** → JSON `null`

#### `AM.FROMJSON <key> <json>`
Create or replace an Automerge document from JSON data. The inverse of `AM.TOJSON`.

```redis
# Create document from JSON
AM.FROMJSON mydoc '{"name":"Alice","age":30,"active":true}'

# Verify the data
AM.GETTEXT mydoc name
# Returns: "Alice"

AM.GETINT mydoc age
# Returns: 30
```

Type conversions:
- JSON objects `{}` → Automerge **Maps**
- JSON arrays `[]` → Automerge **Lists**
- JSON strings → Automerge **text** values
- JSON numbers (integer) → Automerge **integers**
- JSON numbers (float) → Automerge **doubles**
- JSON `true`/`false` → Automerge **booleans**
- JSON `null` → Automerge **null**

Requirements:
- The root JSON value **must be an object** `{}`
- Nested objects and arrays are fully supported
- All standard JSON data types are supported

**Example with nested data:**

```redis
# Import complex JSON structure
AM.FROMJSON config '{"database":{"host":"localhost","port":5432},"features":["api","auth","cache"]}'

# Access nested values
AM.GETTEXT config database.host
# Returns: "localhost"

AM.GETINT config database.port
# Returns: 5432

AM.GETTEXT config features[0]
# Returns: "api"
```

**Roundtrip example:**

```redis
# Create document traditionally
AM.NEW original
AM.PUTTEXT original title "My Document"
AM.CREATELIST original tags
AM.APPENDTEXT original tags "important"
AM.APPENDTEXT original tags "draft"

# Export to JSON
AM.TOJSON original
# Returns: {"title":"My Document","tags":["important","draft"]}

# Import into new document
AM.FROMJSON copy '{"title":"My Document","tags":["important","draft"]}'

# Both documents now have identical content
AM.TOJSON copy
# Returns: {"title":"My Document","tags":["important","draft"]}
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

#### `AM.SPLICETEXT <key> <path> <pos> <del> <text>`
Perform a splice operation on text (insert, delete, or replace characters). This is more efficient than replacing entire strings for small edits.

```redis
# Replace "World" with "Redis" in "Hello World"
AM.SPLICETEXT mydoc greeting 6 5 "Redis"

# Insert " there" at position 5 in "Hello"
AM.SPLICETEXT mydoc greeting 5 0 " there"

# Delete 3 characters starting at position 10
AM.SPLICETEXT mydoc greeting 10 3 ""
```

Parameters:
- `pos` - Starting position (0-indexed)
- `del` - Number of characters to delete
- `text` - Text to insert at position

#### `AM.PUTDIFF <key> <path> <diff>`
Apply a unified diff to update text efficiently. Useful for applying patches from version control systems.

```redis
AM.PUTDIFF mydoc content "--- a/content
+++ b/content
@@ -1 +1 @@
-Hello World
+Hello Redis
"
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

#### `AM.PUTCOUNTER <key> <path> <value>`
Set a counter value. Counters are special CRDT types that support distributed increment operations with proper conflict resolution across multiple clients.

```redis
AM.PUTCOUNTER mydoc stats.views 0
AM.PUTCOUNTER mydoc metrics.requests 1000
```

#### `AM.GETCOUNTER <key> <path>`
Get a counter value.

```redis
AM.GETCOUNTER mydoc stats.views
# Returns: 0
```

#### `AM.INCCOUNTER <key> <path> <delta>`
Increment a counter by the specified delta. Unlike regular integers, counter increments from different clients are automatically merged without conflicts.

```redis
# Increment by positive value
AM.INCCOUNTER mydoc stats.views 1
AM.INCCOUNTER mydoc stats.views 5

# Decrement using negative value
AM.INCCOUNTER mydoc stats.errors -1
```

**Counter vs Integer:**
- **Integers** (`AM.PUTINT`/`AM.GETINT`) - Last write wins. If two clients set different values, one overwrites the other.
- **Counters** (`AM.PUTCOUNTER`/`AM.GETCOUNTER`/`AM.INCCOUNTER`) - Increments merge correctly. If two clients both increment by 1, the final value is +2.

**Use counters for:**
- Page view counters that multiple clients increment
- Like/vote counts from distributed clients
- Request counters across multiple servers
- Any metric that needs correct distributed counting

**Example of counter conflict resolution:**

```redis
# Initial state
AM.PUTCOUNTER doc1 views 0

# Client A increments by 5
AM.INCCOUNTER doc1 views 5

# Client B (offline) increments by 3
AM.INCCOUNTER doc2 views 3

# When synchronized, result is 8 (not 3 or 5)
AM.GETCOUNTER doc1 views
# Returns: 8
```

### Text Marks Operations

Marks provide rich text metadata for text content, allowing you to annotate ranges of text with attributes like formatting, links, comments, or any custom metadata. Marks are ideal for building collaborative rich text editors.

#### `AM.MARKCREATE <key> <path> <name> <value> <start> <end> [expand]`
Create a mark on a text range. Marks annotate character ranges with metadata.

```redis
# Create a bold mark on characters 0-5
AM.MARKCREATE mydoc content bold true 0 5

# Create a link mark with URL
AM.MARKCREATE mydoc content link "https://example.com" 10 20

# Create a comment mark
AM.MARKCREATE mydoc content comment "Fix this typo" 15 19
```

Parameters:
- `name` - Mark identifier (e.g., "bold", "link", "comment")
- `value` - Mark value (string, integer, double, or boolean)
- `start` - Start position (0-indexed, inclusive)
- `end` - End position (0-indexed, exclusive)
- `expand` - (optional) How mark expands with edits: "none", "before", "after", "both" (default: "none")

**Expand behavior:**
- `none` - Mark doesn't expand when text is inserted at boundaries
- `before` - Mark expands when text is inserted before the start
- `after` - Mark expands when text is inserted after the end
- `both` - Mark expands when text is inserted at either boundary

**Automatic Text object conversion:**
If the path contains a simple string scalar, it will be automatically converted to a Text object before applying marks. This allows you to use `AM.PUTTEXT` for initial content, then add marks without manual conversion.

#### `AM.MARKS <key> <path>`
Retrieve all marks on a text field. Returns an array of marks with their names, values, and ranges.

```redis
AM.MARKS mydoc content
# Returns array: ["bold", true, 0, 5, "link", "https://example.com", 10, 20, ...]
```

Each mark is represented as 4 consecutive values:
1. Mark name (string)
2. Mark value (string/int/double/bool)
3. Start position (integer)
4. End position (integer)

#### `AM.MARKCLEAR <key> <path> <name> <start> <end> [expand]`
Remove a mark from a text range.

```redis
# Remove bold mark from characters 0-5
AM.MARKCLEAR mydoc content bold 0 5

# Remove link mark from characters 10-20 with expand behavior
AM.MARKCLEAR mydoc content link 10 20 both
```

Parameters match `AM.MARKCREATE` except no value is needed since we're removing the mark.

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

#### `AM.MAPLEN <key> <path>`
Get the number of keys in a map (object).

```redis
# Get number of keys in root map
AM.MAPLEN mydoc ""

# Get number of keys in nested map
AM.MAPLEN mydoc user
# Returns: 3 (if user has 3 keys like name, age, email)

# Get number of keys in deeply nested map
AM.MAPLEN mydoc config.database
# Returns: number of keys in the database config map
```

**Notes:**
- Returns `0` for an empty map
- Returns `null` if the path doesn't exist
- Counts all keys in the map including nested objects and lists
- Works with both flat keys and nested path syntax
- Supports JSONPath-style `$` prefix

**Example:**

```redis
# Create a document with nested structure
AM.NEW config
AM.PUTTEXT config database.host "localhost"
AM.PUTINT config database.port 5432
AM.PUTTEXT config database.name "mydb"
AM.CREATELIST config features

# Get map length
AM.MAPLEN config ""
# Returns: 2 (database and features)

AM.MAPLEN config database
# Returns: 3 (host, port, name)
```

## Real-Time Synchronization

Redis-Automerge provides built-in support for real-time synchronization using Redis pub/sub.

### Change Notifications

All write operations (`AM.PUTTEXT`, `AM.PUTINT`, `AM.PUTDOUBLE`, `AM.PUTBOOL`, `AM.PUTCOUNTER`, `AM.INCCOUNTER`, `AM.SPLICETEXT`, `AM.APPLY`, etc.) automatically publish changes to a Redis pub/sub channel:

```
Channel: changes:{key}
Message: base64-encoded Automerge change bytes
```

### Subscribing to Changes

Clients can subscribe to document changes using Redis SUBSCRIBE:

```redis
SUBSCRIBE changes:mydoc
```

Or using Webdis WebSocket (`.json` endpoint):

```javascript
const ws = new WebSocket('ws://localhost:7379/.json');
ws.send(JSON.stringify(['SUBSCRIBE', 'changes:mydoc']));
```

### Synchronization Pattern

1. **Client A** makes a change to a document
2. Change is applied locally and sent to server via `AM.APPLY`
3. Server stores the change and publishes to `changes:{key}` channel
4. **Client B** receives change via pub/sub subscription
5. **Client B** applies change locally using `Automerge.applyChanges()`
6. Both clients are now synchronized with automatic conflict resolution

### Loading Document State

New clients can sync by:
1. Load full document: `AM.SAVE {key}` → `Automerge.load(bytes)`
2. Subscribe to changes: `SUBSCRIBE changes:{key}`
3. Apply incremental updates as they arrive

Or use `AM.CHANGES` for differential sync:
1. Get all changes: `AM.CHANGES {key}`
2. Apply changes in order
3. Subscribe for future updates

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

### Analytics with Counters

```redis
# Create analytics document
AM.NEW analytics:page123

# Initialize counters
AM.PUTCOUNTER analytics:page123 views 0
AM.PUTCOUNTER analytics:page123 likes 0
AM.PUTCOUNTER analytics:page123 shares 0

# Multiple clients can increment concurrently
# Client 1:
AM.INCCOUNTER analytics:page123 views 1
AM.INCCOUNTER analytics:page123 likes 1

# Client 2 (simultaneously):
AM.INCCOUNTER analytics:page123 views 1
AM.INCCOUNTER analytics:page123 shares 1

# All increments are properly merged
AM.GETCOUNTER analytics:page123 views
# Returns: 2 (both increments counted)

AM.GETCOUNTER analytics:page123 likes
# Returns: 1

AM.GETCOUNTER analytics:page123 shares
# Returns: 1

# Decrement for corrections
AM.INCCOUNTER analytics:page123 views -1
AM.GETCOUNTER analytics:page123 views
# Returns: 1
```

### JSON Import/Export

```redis
# Import data from external JSON source
AM.FROMJSON api:response '{"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}],"total":2,"page":1}'

# Query the imported data
AM.LISTLEN api:response users
# Returns: 2

AM.GETINT api:response total
# Returns: 2

# Export document to JSON for external use
AM.TOJSON api:response
# Returns: {"users":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}],"total":2,"page":1}

# Export with pretty formatting for debugging
AM.TOJSON api:response true
# Returns formatted JSON:
# {
#   "users": [
#     {
#       "id": 1,
#       "name": "Alice"
#     },
#     {
#       "id": 2,
#       "name": "Bob"
#     }
#   ],
#   "total": 2,
#   "page": 1
# }
```

### Rich Text Editor with Marks

```redis
# Create a document for collaborative rich text editing
AM.NEW doc:article

# Add initial content
AM.PUTTEXT doc:article content "Welcome to our collaborative editor!"

# Apply formatting marks
# Make "Welcome" bold (characters 0-7)
AM.MARKCREATE doc:article content bold true 0 7

# Make "collaborative" italic (characters 15-28)
AM.MARKCREATE doc:article content italic true 15 28

# Add a link to "editor" (characters 29-35)
AM.MARKCREATE doc:article content link "https://example.com/editor" 29 35

# Add a comment mark for review
AM.MARKCREATE doc:article content comment "Great intro!" 0 35

# Retrieve all marks to render in UI
AM.MARKS doc:article content
# Returns: ["bold", true, 0, 7, "italic", true, 15, 28, "link", "https://example.com/editor", 29, 35, "comment", "Great intro!", 0, 35]

# User edits text using splice
AM.SPLICETEXT doc:article content 15 13 "amazing"
# Text is now: "Welcome to our amazing editor!"
# Marks automatically adjust to the new text positions

# Remove the comment after review
AM.MARKCLEAR doc:article content comment 0 35

# Get updated marks
AM.MARKS doc:article content
# Returns: ["bold", true, 0, 7, "link", "https://example.com/editor", 23, 29]
# Note: positions adjusted after the splice operation

# Multiple users can add marks simultaneously
# User A adds highlight
AM.MARKCREATE doc:article content highlight "yellow" 8 11

# User B adds font size (with expand behavior)
AM.MARKCREATE doc:article content fontSize 16 0 7 both

# All marks merge automatically with CRDT conflict resolution
AM.MARKS doc:article content
# Returns all marks with proper conflict-free merging
```

### Marks with Expand Behavior

```redis
# Create document with expandable marks
AM.NEW doc:notes
AM.PUTTEXT doc:notes text "Hello World"

# Create a bold mark that expands on both sides
AM.MARKCREATE doc:notes text bold true 0 5 both

# Insert text at the beginning (position 0)
AM.SPLICETEXT doc:notes text 0 0 "**"
# Mark expands: now covers "**Hello" (0-7)

# Insert text at the end of the mark
AM.SPLICETEXT doc:notes text 7 0 "**"
# Mark expands: now covers "**Hello**" (0-9)

# Get marks to verify expansion
AM.MARKS doc:notes text
# Returns: ["bold", true, 0, 9]

# Create a mark that doesn't expand
AM.MARKCREATE doc:notes text code true 10 15 none

# Insert at boundaries - mark doesn't expand
AM.SPLICETEXT doc:notes text 10 0 "["
AM.SPLICETEXT doc:notes text 16 0 "]"

# Code mark stays at original range (now 11-16 after insertions)
AM.MARKS doc:notes text
# Returns: ["bold", true, 0, 9, "code", true, 11, 16]
```

### Collaborative Annotations

```redis
# Create a shared document for team collaboration
AM.NEW doc:proposal
AM.PUTTEXT doc:proposal content "We propose to implement the new feature by Q2 2024."

# Team member adds suggestion
AM.MARKCREATE doc:proposal content suggestion "Consider Q1 instead?" 43 50

# Another member adds approval mark
AM.MARKCREATE doc:proposal content approved true 0 50

# Project manager highlights key section
AM.MARKCREATE doc:proposal content priority "high" 15 39

# Export to JSON for external tools
AM.TOJSON doc:proposal true

# All marks are preserved and can be synced across clients
# via Redis pub/sub (changes:doc:proposal channel)
```

## Search Indexing (RediSearch Integration)

The redis-automerge module provides automatic indexing of Automerge document fields to enable full-text search via [RediSearch](https://redis.io/docs/interact/search-and-query/). When configured, the module automatically creates and maintains shadow index documents (prefixed with `am:idx:`) that mirror specified fields from your Automerge documents.

### Index Formats

The module supports two indexing formats:

1. **Hash Format** (default): Creates Redis Hash keys with flattened field names. Best for simple text search with minimal overhead.
2. **JSON Format**: Creates RedisJSON documents with nested structure preserved. Best for complex queries, array search, and type preservation.

### How It Works

1. **Configuration**: Define which key patterns to index, which paths to extract, and the index format
2. **Automatic Updates**: When you modify indexed documents (via `AM.PUTTEXT`, `AM.FROMJSON`, etc.), shadow index documents are automatically updated
3. **Search with RediSearch**: Use `FT.CREATE` to create a RediSearch index on the shadow documents, then query with `FT.SEARCH`

### Commands

#### AM.INDEX.CONFIGURE

Configure indexing for a key pattern.

**Syntax:**
```
AM.INDEX.CONFIGURE pattern [--format hash|json] path [path ...]
```

**Parameters:**
- `pattern`: Key pattern to match (e.g., `"article:*"`, `"user:*"`)
- `--format`: Index format, either `hash` (default) or `json`
- `path`: One or more paths to extract from documents

**Examples:**

Hash format (default):
```bash
# Index title and content fields as Hash
AM.INDEX.CONFIGURE "article:*" title content author.name

# Explicit Hash format
AM.INDEX.CONFIGURE "user:*" --format hash name email profile.bio
```

JSON format (requires RedisJSON):
```bash
# Index as JSON document with nested structure preserved
AM.INDEX.CONFIGURE "product:*" --format json title price description tags

# Index with nested paths - creates nested JSON structure
AM.INDEX.CONFIGURE "book:*" --format json title author.name author.country price
```

#### AM.INDEX.ENABLE

Enable indexing for a previously configured pattern.

**Syntax:**
```
AM.INDEX.ENABLE pattern
```

**Example:**
```bash
AM.INDEX.ENABLE "article:*"
```

#### AM.INDEX.DISABLE

Temporarily disable indexing for a pattern (without removing the configuration).

**Syntax:**
```
AM.INDEX.DISABLE pattern
```

**Example:**
```bash
AM.INDEX.DISABLE "article:*"
```

#### AM.INDEX.REINDEX

Manually rebuild the shadow Hash for a specific key.

**Syntax:**
```
AM.INDEX.REINDEX key
```

**Returns:** `1` if index was updated, `0` if no matching configuration or no indexable fields

**Example:**
```bash
AM.INDEX.REINDEX article:123
```

#### AM.INDEX.STATUS

Show indexing configuration for one or all patterns.

**Syntax:**
```
AM.INDEX.STATUS [pattern]
```

**Example:**
```bash
# Show all configurations
AM.INDEX.STATUS

# Show specific pattern
AM.INDEX.STATUS "article:*"
```

### Hash vs JSON Format

#### Hash Format

**Pros:**
- No additional dependencies
- Lower memory overhead
- Simple flat key-value structure
- Fast for basic text search

**Cons:**
- Flattened field names (nested paths use underscores)
- All values stored as text strings
- Cannot index arrays/lists
- No type preservation (numbers, booleans become strings)

**Field Naming:**
- Nested paths flattened with underscores: `author.name` → `author_name`
- Array notation removed: `items[0]` → `items_0`

**Example:**
```bash
AM.INDEX.CONFIGURE "article:*" title content author.name

# Creates Hash like:
# HGETALL am:idx:article:123
# 1) "title"
# 2) "Introduction to CRDTs"
# 3) "content"
# 4) "Conflict-free replicated data types..."
# 5) "author_name"
# 6) "Alice"
```

#### JSON Format

**Pros:**
- Preserves nested structure
- Type preservation (int, double, bool)
- Can index arrays for multi-value search
- Complex queries with JSON path syntax
- Better for structured data

**Cons:**
- Requires RedisJSON module
- Higher memory overhead
- Slightly slower updates

**Requirements:**
- RedisJSON module must be loaded: `redis-server --loadmodule /path/to/rejson.so`
- Available in Redis Stack or as standalone module

**Structure Preservation:**
- Nested paths create nested objects: `author.name` → `{"author": {"name": "Alice"}}`
- Arrays preserved: `tags` → `["rust", "redis", "crdt"]`
- Types preserved: integers, doubles, booleans maintain their types

**Example:**
```bash
AM.INDEX.CONFIGURE "product:*" --format json title price tags

# Creates JSON document like:
# JSON.GET am:idx:product:laptop
# {
#   "title": "ThinkPad X1",
#   "price": 1299,
#   "tags": ["business", "portable"]
# }
```

### Complete Examples

#### Example 1: Hash Format (Basic Text Search)

```bash
# 1. Configure indexing for article keys
AM.INDEX.CONFIGURE "article:*" title content author tags

# 2. Create some articles
AM.NEW article:1
AM.PUTTEXT article:1 title "Introduction to CRDTs"
AM.PUTTEXT article:1 content "Conflict-free Replicated Data Types are amazing..."
AM.PUTTEXT article:1 author "Alice"

AM.NEW article:2
AM.PUTTEXT article:2 title "Redis and Automerge"
AM.PUTTEXT article:2 content "Building real-time applications with Redis..."
AM.PUTTEXT article:2 author "Bob"

# Shadow Hashes are automatically created:
# - am:idx:article:1 with fields: title, content, author
# - am:idx:article:2 with fields: title, content, author

# 3. Create a RediSearch index on the shadow Hashes
FT.CREATE idx:articles ON HASH PREFIX 1 am:idx:article: SCHEMA title TEXT content TEXT author TEXT

# 4. Search using RediSearch
FT.SEARCH idx:articles "CRDT"
# Returns: article:1

FT.SEARCH idx:articles "@author:Bob"
# Returns: article:2

FT.SEARCH idx:articles "real-time Redis"
# Returns: article:2

# 5. Check indexing status
AM.INDEX.STATUS "article:*"
# Output:
# pattern: article:*
# enabled: true
# paths: title, content, author, tags
```

#### Example 2: JSON Format (Structured Data with Arrays)

```bash
# 1. Configure JSON indexing for product keys
AM.INDEX.CONFIGURE "product:*" --format json title price inStock tags description

# 2. Create some products
AM.NEW product:laptop
AM.PUTTEXT product:laptop title "ThinkPad X1 Carbon"
AM.PUTINT product:laptop price 1299
AM.PUTBOOL product:laptop inStock true
AM.CREATELIST product:laptop tags
AM.APPENDTEXT product:laptop tags "business"
AM.APPENDTEXT product:laptop tags "portable"
AM.APPENDTEXT product:laptop tags "lightweight"
AM.PUTTEXT product:laptop description "Professional ultrabook for business users"

AM.NEW product:phone
AM.PUTTEXT product:phone title "iPhone 15"
AM.PUTINT product:phone price 999
AM.PUTBOOL product:phone inStock false
AM.CREATELIST product:phone tags
AM.APPENDTEXT product:phone tags "smartphone"
AM.APPENDTEXT product:phone tags "5G"
AM.PUTTEXT product:phone description "Latest iPhone with advanced features"

# Shadow JSON documents are automatically created:
# JSON.GET am:idx:product:laptop
# {
#   "title": "ThinkPad X1 Carbon",
#   "price": 1299,
#   "inStock": true,
#   "tags": ["business", "portable", "lightweight"],
#   "description": "Professional ultrabook for business users"
# }

# 3. Create a RediSearch index on the JSON documents
FT.CREATE idx:products ON JSON PREFIX 1 am:idx:product: SCHEMA \
  $.title AS title TEXT \
  $.price AS price NUMERIC \
  $.inStock AS inStock TAG \
  $.tags[*] AS tags TAG \
  $.description AS description TEXT

# 4. Search using RediSearch with JSON
# Find laptops (text search in description)
FT.SEARCH idx:products "@description:laptop"

# Find products under $1000
FT.SEARCH idx:products "@price:[0 1000]"

# Find in-stock items
FT.SEARCH idx:products "@inStock:{true}"

# Find products with "portable" tag
FT.SEARCH idx:products "@tags:{portable}"

# Combined query: portable items under $1500
FT.SEARCH idx:products "@tags:{portable} @price:[0 1500]"
```

#### Example 3: JSON Format with Nested Structure

```bash
# Configure indexing with nested paths
AM.INDEX.CONFIGURE "book:*" --format json title author.name author.country publisher.name price

# Create a book
AM.NEW book:rust101
AM.PUTTEXT book:rust101 title "The Rust Programming Language"
AM.PUTTEXT book:rust101 author.name "Steve Klabnik"
AM.PUTTEXT book:rust101 author.country "USA"
AM.PUTTEXT book:rust101 publisher.name "No Starch Press"
AM.PUTINT book:rust101 price 39

# Creates nested JSON structure:
# JSON.GET am:idx:book:rust101
# {
#   "title": "The Rust Programming Language",
#   "author": {
#     "name": "Steve Klabnik",
#     "country": "USA"
#   },
#   "publisher": {
#     "name": "No Starch Press"
#   },
#   "price": 39
# }

# Create RediSearch index with nested paths
FT.CREATE idx:books ON JSON PREFIX 1 am:idx:book: SCHEMA \
  $.title AS title TEXT \
  $.author.name AS author TEXT \
  $.author.country AS country TAG \
  $.publisher.name AS publisher TEXT \
  $.price AS price NUMERIC

# Search by author
FT.SEARCH idx:books "@author:Klabnik"

# Search by country
FT.SEARCH idx:books "@country:{USA}"
```

### Field Naming

**Hash format:** Nested paths are flattened with underscores:
- `author.name` → `author_name`
- `profile.location` → `profile_location`

**JSON format:** Nested paths create nested objects:
- `author.name` → `{"author": {"name": "..."}}`
- `profile.location` → `{"profile": {"location": "..."}}`

### Key Patterns

The module supports simple wildcard patterns:
- `article:*` - Matches all keys starting with `article:`
- `user:*` - Matches all keys starting with `user:`
- `*` - Matches all keys (use with caution)

### Automatic Updates

Shadow index documents (Hash or JSON) are automatically updated when you:
- Modify fields: `AM.PUTTEXT`, `AM.PUTINT`, `AM.PUTDOUBLE`, `AM.PUTBOOL`
- Append to lists: `AM.APPENDTEXT`, `AM.APPENDINT`, etc.
- Import JSON: `AM.FROMJSON`
- Apply changes: `AM.APPLY` (in some cases; use `AM.INDEX.REINDEX` after bulk operations)

Shadow index documents are **not** automatically created when:
- Loading documents: `AM.LOAD` (use `AM.INDEX.REINDEX` afterward)
- Creating new documents: `AM.NEW` (wait until fields are populated)

### Performance Considerations

- Indexing is asynchronous and errors don't fail write operations
- Only configured paths are indexed
- Shadow documents are updated on every write to indexed fields
- Use `AM.INDEX.DISABLE` during bulk imports, then `AM.INDEX.REINDEX` afterward
- Configure indexing for specific patterns rather than using `*` to minimize overhead
- JSON format has slightly higher overhead than Hash format but provides more features
- Consider your use case: Hash for simple text search, JSON for complex queries with arrays and types

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
- **Latest docs**: [`https://michelp.github.io/redis-automerge/`](https://michelp.github.io/redis-automerge/`)

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
4. **Text Operations** - Efficient splice and diff operations for text editing
5. **List Operations** - Create lists, append values, get length
6. **Change Management** - Track and retrieve document changes for synchronization
7. **Pub/Sub Integration** - Automatic change notifications via Redis channels
8. **Persistence** - RDB save/load and AOF change tracking
9. **Replication** - Change propagation to Redis replicas

### Synchronization Flow

```
┌─────────────┐         ┌──────────────┐         ┌─────────────┐
│  Client A   │         │    Redis     │         │  Client B   │
│  (Browser)  │         │ + Module     │         │  (Browser)  │
└──────┬──────┘         └──────┬───────┘         └──────┬──────┘
       │                       │                        │
       │  1. Local Edit        │                        │
       │─────────────────────> │                        │
       │  AM.SPLICETEXT        │                        │
       │                       │                        │
       │  2. Change Published  │                        │
       │                       ├────────────────────────>
       │                       │  PUBLISH changes:doc   │
       │                       │                        │
       │                       │  3. Apply Change       │
       │                       │                        │
       │                       │ <──────────────────────│
       │                       │                        │
       │  4. Both Synced       │                        │
```

## Resources

- [Automerge Documentation](https://automerge.org/)
- [Redis Module API](https://redis.io/topics/modules-intro)
- [RedisJSON](https://redis.io/docs/stack/json/) - Similar path syntax reference
