# Shareable Link Editor Demo

A real-time collaborative text editor with shareable links, demonstrating Automerge CRDT synchronization through Redis-Automerge.

## Overview

This demo extends the collaborative editor with:
- **Shareable Links**: Create rooms with unique URLs that others can join
- **Real-Time Room List**: See all available rooms with active user counts
- **Random or Custom Room Names**: Generate random IDs or choose your own
- **URL-Based Joining**: Share links that auto-select the room

## Features

### Room Management
- **Create Random Room**: Generate an 8-character room ID automatically
- **Create Custom Room**: Choose your own room name (alphanumeric, dash, underscore)
- **Room List**: See all existing rooms with active user counts
- **Real-Time Updates**: Room list updates automatically when rooms are created/deleted

### Collaborative Editing
- **Single Editor Per Page**: Clean, focused editing experience
- **Real-Time Sync**: Changes propagate instantly to all connected users
- **Cursor Preservation**: Your cursor stays in the right place as others edit
- **Conflict-Free Merging**: Concurrent edits merge automatically via Automerge CRDT

## Architecture

### Module Structure

**EditorCore** - Shared primitives:
- Connection management (`checkConnection`, `setupWebSocket`)
- Document operations (`initializeDocument`, `applySpliceToServer`)
- Utilities (`calculateSplice`, `debounce`, `log`)

**ShareableMode** - Room management:
- Room CRUD operations
- Real-time room list via keyspace notifications
- Single editor with WebSocket sync
- URL parameter parsing and shareable link generation

### Data Flow

```
Create/Join Room
    ↓
Initialize Automerge Document
    ↓
Subscribe to changes:{roomKey} channel
    ↓
User Types → Calculate Diff → AM.SPLICETEXT → Server
    ↓
Server Publishes Change Bytes
    ↓
All Subscribers Receive Changes
    ↓
Apply Changes → Update Editor → Preserve Cursor
```

### Redis Integration

**Document Storage**:
- Key pattern: `am:room:{roomName}`
- Type: Custom Redis type (REDIS_AUTOMERGE_TYPE)
- Commands: `AM.NEW`, `AM.GETTEXT`, `AM.SPLICETEXT`

**Change Notifications**:
- Channel: `changes:am:room:{roomName}`
- Server publishes Automerge change bytes when document modified

**Room List Tracking**:
- Keyspace pattern: `__keyspace@0__:am:room:*`
- Events: `am.new`, `del`, `expired`
- Real-time notifications when rooms created/deleted

**Active Users**:
- Command: `PUBSUB NUMSUB changes:am:room:{roomName}`
- Returns count of WebSocket subscribers to room's channel

## Usage

### Step 1: Start Services

```bash
docker compose up -d redis webdis
```

Ensure Redis has keyspace notifications enabled (see `docker-compose.yml`).

### Step 2: Open Demo

Open `demo/editor.html` in your browser.

### Step 3: Create or Join Room

**Option A: Create Random Room**
1. Click "Shareable Link Mode" tab
2. Click "Create Random Room"
3. Room created with 8-character ID (e.g., `a7b3c9d2`)

**Option B: Create Custom Room**
1. Enter custom name in text field (e.g., `my-collab-doc`)
2. Click "Create Custom Room"

**Option C: Join Existing Room**
1. Click "Refresh Room List"
2. Click on any room in the list
3. Join room and start editing

**Option D: Join via URL**
1. Receive shareable link from another user
2. Open link (e.g., `http://localhost:8080/demo/editor.html?room=my-room`)
3. Room auto-selected, click to join

### Step 4: Share Link

1. Once in a room, click "Copy Link" button
2. Share with others via email, chat, etc.
3. Others can join same room and collaborate in real-time

### Step 5: Collaborate

- Type in the editor
- Watch changes sync to other users' browsers
- See character count and version number update
- Active user count shown in room list

## Testing Scenarios

### Scenario 1: Room Creation
1. Create random room → Verify unique ID generated
2. Create custom room → Verify name accepted
3. Try duplicate name → Verify prompt to join instead

### Scenario 2: Multi-User Collaboration
1. User A creates room, copies link
2. User B opens link in new tab/window
3. User A types "Hello"
4. User B sees "Hello" appear
5. User B types " World"
6. User A sees "Hello World"

### Scenario 3: Room List Updates
1. User A creates room "test-room"
2. User B refreshes room list
3. Verify "test-room" appears with 1 active user
4. User B joins room
5. Verify active user count updates to 2

### Scenario 4: Cursor Preservation
1. Two users in same room
2. User A places cursor mid-text
3. User B edits before cursor position
4. Verify User A's cursor adjusts correctly

### Scenario 5: Room Persistence
1. Create room, add content
2. Disconnect (close browser)
3. Rejoin room via URL
4. Verify content persists from Redis

## Configuration

### Redis Keyspace Notifications

Required in `docker-compose.yml`:

```yaml
command: >
  redis-server
  --loadmodule /usr/lib/redis/modules/libredis_automerge.so
  --notify-keyspace-events KEA
```

**Flags**:
- `K`: Keyspace events
- `E`: Keyevent events
- `A`: All commands

### Webdis

WebSocket endpoint: `ws://localhost:7379/.json`

Used for:
- Pub/Sub subscriptions (`SUBSCRIBE`, `PSUBSCRIBE`)
- Real-time change notifications

## Limitations

1. **Room List Scaling**: Uses `KEYS` for simplicity (use `SCAN` in production)
2. **No Authentication**: Anyone with link can join room
3. **No Room Deletion UI**: Rooms persist until manually deleted via Redis CLI
4. **Base64 Overhead**: Binary data encoded as base64 (~33% size increase)
5. **Single Text Field**: Only `text` field supported (no rich text, lists, etc.)

## Future Enhancements

### User Presence
Show who's currently editing:
```javascript
{
  users: {
    "peer-abc123": {
      name: "Alice",
      cursor: 42,
      lastSeen: timestamp
    }
  }
}
```

### Room Passwords
Add optional password protection for private rooms.

### Room Expiry
Auto-delete rooms with no activity after N hours.

### Rich Text Support
Use Automerge `Text` type for better character-level operations:
```javascript
doc.content = new Automerge.Text();
doc.content.insertAt(0, "Hello");
```

### Presence Indicators
Show live cursor positions of other users.

## Related Documentation

- [Dual-Pane Editor](COLLABORATIVE_EDITOR.md)
- [Automerge Documentation](https://automerge.org/docs/hello/)
- [Redis Keyspace Notifications](https://redis.io/docs/manual/keyspace-notifications/)
- [Redis Pub/Sub](https://redis.io/docs/manual/pubsub/)
