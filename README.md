# Redis-Automerge

[![CI](https://github.com/michelp/redis-automerge/actions/workflows/ci.yml/badge.svg)](https://github.com/michelp/redis-automerge/actions/workflows/ci.yml)
[![Documentation](https://github.com/michelp/redis-automerge/actions/workflows/docs.yml/badge.svg)](https://github.com/michelp/redis-automerge/actions/workflows/docs.yml)

A Redis module that integrates [Automerge](https://automerge.org/) CRDT (Conflict-free Replicated Data Type) documents into Redis, providing JSON-like document storage with automatic conflict resolution.

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
