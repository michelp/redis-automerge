# Collaborative Editor Demo

A real-time collaborative text editor demonstrating Automerge CRDT synchronization with Redis-Automerge.

## Overview

This demo showcases the core features of Conflict-free Replicated Data Types (CRDTs) through a side-by-side collaborative editor. Two editor panes share the same Automerge document, synchronizing changes in real-time through Redis.

## Features

### Local-First Architecture
- **Offline Capability**: Each editor maintains its own local Automerge document
- **Instant UI Updates**: Changes appear immediately in the local editor
- **Background Sync**: Changes sync to Redis and other peers asynchronously

### CRDT Synchronization
- **Conflict-Free Merging**: Concurrent edits merge automatically without conflicts
- **Causal Ordering**: Changes are applied in the correct order
- **Eventual Consistency**: All peers eventually converge to the same state

### Real-Time Features
- **Automatic Sync**: Changes propagate between editors automatically
- **Change Tracking**: Visual feedback showing sync events
- **Version History**: Track document evolution with change counts

## How It Works

### 1. Document Structure

Each editor maintains an Automerge document:

```javascript
{
  text: "Content goes here..."
}
```

### 2. Change Detection

When a user types:
1. Local Automerge document is updated
2. Changes are extracted: `Automerge.getChanges(oldDoc, newDoc)`
3. Changes are broadcast to Redis
4. Document is saved to Redis

### 3. Synchronization Flow

```
Editor A                Redis Pub/Sub           Editor B
   |                      |                      |
   |                  [WebSocket]            [WebSocket]
   |                      |                      |
   |-- Edit detected ---->|                      |
   |-- Save full doc ---->|                      |
   |-- PUBLISH changes -->|                      |
   |                      |                      |
   |                      |---- SUBSCRIBE ------>|
   |                      |      message         |
   |                      |                      |-- Apply changes
   |                      |                      |-- Update UI
```

### 4. Conflict Resolution

Automerge CRDT automatically handles conflicts:

**Scenario**: Both editors type at the same time
- Editor A types: "Hello"
- Editor B types: "World"

**Result**: Changes merge without conflict
- Final text contains both changes
- Order is deterministic based on Automerge's algorithm
- No data loss

## Usage

### Step 1: Create Document

1. Enter a document key (e.g., `my-collab-doc`)
2. Click **Create New Document**
3. Empty Automerge documents are initialized locally
4. Document is created in Redis

### Step 2: Connect Editors

1. Click **Connect Editors**
2. WebSocket connections established to Redis
3. Both editors subscribe to the sync channel
4. Status changes to "Syncing ✓"

### Step 3: Edit Collaboratively

1. Type in either editor
2. Watch changes sync to the other editor
3. Try typing in both simultaneously to see conflict-free merging

### Step 4: Observe Sync

- **Sync Log**: Shows all sync events with timestamps
- **Local State**: Displays character count, change count, peer ID
- **Version Numbers**: Track document evolution

## Technical Details

### Automerge Integration

Uses the official Automerge JavaScript library:

```javascript
// Initialize document
let doc = Automerge.init();

// Make changes
doc = Automerge.change(doc, d => {
    d.text = "New content";
});

// Extract changes
const changes = Automerge.getChanges(oldDoc, newDoc);

// Apply changes
doc = Automerge.applyChanges(doc, changes);

// Merge documents
doc = Automerge.merge(localDoc, remoteDoc);
```

### Redis Integration

Documents are stored in Redis using the redis-automerge module:

```javascript
// Save to Redis
const saved = Automerge.save(doc);
await fetch(`${WEBDIS_URL}/SET/automerge:${docKey}/${base64Data}`);

// Load from Redis
const response = await fetch(`${WEBDIS_URL}/GET/automerge:${docKey}`);
const doc = Automerge.load(binaryData);
```

### Synchronization Strategy

**Current Implementation**:
- **WebSocket Pub/Sub**: Real-time push notifications via Webdis WebSocket interface
- **Incremental Changes**: Only Automerge changes are sent over pub/sub, not full documents
- **Full State Persistence**: Complete document saved to Redis for new peers to load initial state
- **Shared Document History**: Both editors initialized from the same base document via `Automerge.clone()`

**Data Flow**:
1. User types in an editor
2. Local Automerge document updated immediately (instant UI)
3. Changes extracted with `Automerge.getChanges()`
4. Full document saved to Redis via `SET` (for persistence)
5. Changes published to Redis Pub/Sub channel via `PUBLISH`
6. Other editors receive via WebSocket `SUBSCRIBE`
7. Changes applied with `Automerge.applyChanges()`
8. Remote editor UI updates

**Production Enhancements**:
- Change compression: Batch multiple rapid changes
- Presence detection: Show active users
- Cursor synchronization: Share cursor positions
- Optimistic UI: Show pending changes differently

## Architecture

```
┌─────────────────┐           ┌─────────────────┐
│   Editor A      │           │   Editor B      │
│                 │           │                 │
│  ┌───────────┐  │           │  ┌───────────┐  │
│  │ Automerge │  │           │  │ Automerge │  │
│  │  Document │  │           │  │  Document │  │
│  └─────┬─────┘  │           │  └─────┬─────┘  │
│        │        │           │        │        │
└────────┼────────┘           └────────┼────────┘
         │                             │
         │         ┌─────────┐         │
         └────────>│  Redis  │<────────┘
                   │ + Webdis│
                   └─────┬───┘
                         │
                   ┌─────┴─────┐
                   │  redis-   │
                   │ automerge │
                   │  module   │
                   └───────────┘
```

## Limitations (Current Demo)

1. **Single Page Instance**: Both editors are in the same browser tab (for demo purposes)
2. **Base64 Encoding**: Binary data encoded as base64 adds ~33% overhead
3. **No Conflict Visualization**: Doesn't highlight merged changes
4. **Full Document Save**: Entire document saved on each edit (incremental changes only sent via pub/sub)
5. **No Presence Awareness**: Doesn't show which peer is currently editing

## Future Enhancements

### Multi-Tab/Multi-User Support
Enable true multi-user collaboration:
- Open demo in multiple browser tabs or windows
- Each tab gets unique peer ID
- Changes sync across all instances
- Test with multiple users on different computers

### Optimized Persistence
Reduce redundant saves:
- Only save to Redis periodically (e.g., every 5 seconds) instead of on every edit
- Rely on pub/sub for real-time sync
- Background persistence for recovery

### Presence Awareness
Show who's editing:
```javascript
{
  peers: {
    "peer-abc123": {
      name: "Alice",
      cursor: { line: 5, col: 12 },
      lastSeen: timestamp
    }
  }
}
```

### Rich Text Support
Use Automerge's text CRDT for better text editing:
```javascript
doc = Automerge.change(doc, d => {
    d.content = new Automerge.Text();
    d.content.insertAt(0, "Hello");
});
```

### Undo/Redo
Leverage Automerge history:
```javascript
const history = Automerge.getHistory(doc);
doc = Automerge.load(history[previousIndex].snapshot);
```

## Testing Scenarios

### Scenario 1: Concurrent Edits
1. Type "Hello" in Editor A
2. Immediately type "World" in Editor B
3. Observe both changes merge

### Scenario 2: Document Recovery
1. Edit in Editor A
2. Disconnect (stop services)
3. Reconnect
4. Changes persist from Redis

### Scenario 3: Rapid Changes
1. Type quickly in Editor A
2. Watch debounced sync in logs
3. See batched updates

## Debugging

### Check Automerge State
Open browser console:
```javascript
// View document
console.log(leftDoc);

// View history
console.log(Automerge.getHistory(leftDoc));

// Export document
console.log(Automerge.save(leftDoc));
```

### Monitor Network
- Open DevTools → Network tab
- Filter for Webdis requests
- Check for failed requests

### View Logs
- Sync log shows all events
- Color-coded by source (left/right/server)
- Timestamps for debugging sync timing

## Related Documentation

- [Automerge Documentation](https://automerge.org/docs/hello/)
- [CRDT Explained](https://crdt.tech/)
- [Redis Pub/Sub](https://redis.io/docs/manual/pubsub/)
- [Main Demo](README.md)
