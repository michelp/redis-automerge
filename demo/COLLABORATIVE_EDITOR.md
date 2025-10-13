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

### 2. Change Detection (Updated - Now Using AM.SPLICETEXT)

When a user types:
1. **Calculate minimal diff**: Compare old and new text to find the minimal change (position, deletions, insertions)
2. **Local Automerge document updated**: Changes applied immediately for instant UI feedback
3. **Incremental splice sent to server**: Only the changed portion via `AM.SPLICETEXT` (position, delete count, text)
4. **Server publishes change**: Automerge change bytes broadcast to all subscribers
5. **Cursor position preserved**: Intelligent cursor adjustment based on where the change occurred

### 3. Synchronization Flow (Updated - AM.SPLICETEXT)

```
Editor A                Redis Pub/Sub           Editor B
   |                      |                      |
   |                  [WebSocket]            [WebSocket]
   |                      |                      |
   |-- Edit detected ---->|                      |
   |-- Calculate diff     |                      |
   |-- AM.SPLICETEXT ---->|                      |
   |   (pos,del,text)     |                      |
   |                      |-- publishes -------->|
   |                      |   change bytes       |
   |                      |                      |-- Apply changes
   |                      |                      |-- Adjust cursor
   |                      |                      |-- Update UI
```

**Key Improvements:**
- Only changed text is sent (not the entire document)
- Cursor position intelligently preserved on both local and remote editors
- Reduced network bandwidth and server processing

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

**Data Flow (Updated with AM.SPLICETEXT)**:
1. User types in an editor
2. Local Automerge document updated immediately (instant UI)
3. **Diff calculated**: Compare previous and current text to find minimal change
4. **Incremental update**: Send only changed portion via `AM.SPLICETEXT` (position, delete count, insert text)
5. Server applies splice operation to its Automerge document
6. Server publishes Automerge change bytes to Redis Pub/Sub channel
7. Other editors receive change bytes via WebSocket `SUBSCRIBE`
8. Changes applied with `Automerge.applyChanges()`
9. **Cursor adjusted**: Remote editor cursor position intelligently updated based on change location
10. Remote editor UI updates with preserved cursor position

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
4. **No Presence Awareness**: Doesn't show which peer is currently editing
5. **Debounced Updates**: 300ms delay before sending changes (prevents excessive updates during fast typing)

## Cursor Position Preservation

The editor now intelligently preserves cursor position when remote changes are applied:

### How It Works

1. **Track cursor position**: Before applying remote changes, the editor saves the current cursor position
2. **Calculate the splice**: Determine what changed (position, deleted characters, inserted text)
3. **Adjust cursor intelligently**:
   - If cursor is **before** the change: Keep cursor in same position
   - If cursor is **inside** the deleted range: Move to end of inserted text
   - If cursor is **after** the change: Shift by the net change in length

### Example Scenarios

**Scenario 1: Remote edit before cursor**
```
Before: "Hello |World"  (cursor at position 6)
Change: Insert "Beautiful " at position 6
After:  "Hello Beautiful |World"  (cursor moves to position 16)
```

**Scenario 2: Remote edit after cursor**
```
Before: "Hello| World"  (cursor at position 5)
Change: Insert "Beautiful " at position 6
After:  "Hello| Beautiful World"  (cursor stays at position 5)
```

**Scenario 3: Cursor inside deleted text**
```
Before: "Hello Wor|ld"  (cursor at position 9)
Change: Delete "World" and insert "Rust" at position 6
After:  "Hello Rust|"  (cursor moves to end of insertion at position 10)
```

This ensures a smooth editing experience where your cursor doesn't jump unexpectedly when others are editing the same document.

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

### Scenario 4: Cursor Preservation
1. Type "Hello World" in Editor A
2. Place cursor in middle of "World" in Editor A (e.g., "Hello Wor|ld")
3. In Editor B, edit text before the cursor (e.g., insert "Beautiful " between "Hello" and "World")
4. Observe that Editor A's cursor adjusts correctly and stays in a sensible position
5. Try editing after the cursor in Editor B - cursor in Editor A should remain stable

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
