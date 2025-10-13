# Shareable Editor Implementation Plan

> **For Claude:** Use `${SUPERPOWERS_SKILLS_ROOT}/skills/collaboration/executing-plans/SKILL.md` to implement this plan task-by-task.

**Goal:** Add a shareable link collaborative editor mode to the existing demo, with real-time room list, single editor per page, and URL-based room joining.

**Architecture:** Module-based refactoring with three namespaces (EditorCore, DualPaneMode, ShareableMode). EditorCore extracts shared primitives from the current implementation. ShareableMode adds room management with Redis keyspace notifications for real-time updates, PUBSUB NUMSUB for active user tracking, and URL parameter support for direct room access.

**Tech Stack:** JavaScript (ES6 modules pattern), Automerge.js, Redis Pub/Sub via Webdis WebSocket, Redis keyspace notifications, HTML/CSS tabs UI

---

## Task 1: Enable Redis Keyspace Notifications

**Files:**
- Modify: `docker-compose.yml`

**Step 1: Update Redis command to enable keyspace events**

In `docker-compose.yml`, find the redis service and update the command to include keyspace notification configuration:

```yaml
services:
  redis:
    image: redis:latest
    ports:
      - "6379:6379"
    volumes:
      - ./redis-automerge/target/release:/usr/lib/redis/modules
    command: >
      redis-server
      --loadmodule /usr/lib/redis/modules/libredis_automerge.so
      --notify-keyspace-events KEA
```

The `KEA` flags enable:
- K: Keyspace events (published as `__keyspace@0__:key`)
- E: Keyevent events (published as `__keyevent@0__:command`)
- A: All commands

**Step 2: Restart Docker services to apply changes**

Run:
```bash
docker compose down
docker compose up -d redis webdis
```

Expected: Services restart with keyspace notifications enabled

**Step 3: Verify keyspace notifications are working**

Run:
```bash
redis-cli CONFIG GET notify-keyspace-events
```

Expected output:
```
1) "notify-keyspace-events"
2) "KEA"
```

**Step 4: Commit**

```bash
git add docker-compose.yml
git commit -m "feat: enable Redis keyspace notifications for room list tracking"
```

---

## Task 2: Refactor EditorCore Namespace

**Files:**
- Modify: `demo/editor-app.js:1-712`

**Step 1: Extract EditorCore namespace with shared utilities**

At the top of `demo/editor-app.js` (before existing code), add the EditorCore namespace:

```javascript
// ============================================================================
// EditorCore - Shared primitives used by both modes
// ============================================================================
const EditorCore = {
    WEBDIS_URL: 'http://localhost:7379',
    WEBDIS_WS_URL: 'ws://localhost:7379',

    /**
     * Generate a unique peer ID for this editor instance.
     * @returns {string} A unique peer ID string
     */
    generatePeerId() {
        return 'peer-' + Math.random().toString(36).substring(2, 15);
    },

    /**
     * Debounce function to limit how often a function can be called.
     * @param {Function} func - The function to debounce
     * @param {number} wait - Milliseconds to wait before calling func
     * @returns {Function} Debounced version of func
     */
    debounce(func, wait) {
        let timeout;
        return function executedFunction(...args) {
            const later = () => {
                clearTimeout(timeout);
                func(...args);
            };
            clearTimeout(timeout);
            timeout = setTimeout(later, wait);
        };
    },

    /**
     * Calculate the difference between two strings and return splice operation parameters.
     * @param {string} oldText - The previous text
     * @param {string} newText - The new text
     * @returns {Object|null} Object with {pos, del, text} or null if no change
     */
    calculateSplice(oldText, newText) {
        // Find common prefix
        let prefixLen = 0;
        const minLen = Math.min(oldText.length, newText.length);
        while (prefixLen < minLen && oldText[prefixLen] === newText[prefixLen]) {
            prefixLen++;
        }

        // Find common suffix
        let suffixLen = 0;
        const oldEnd = oldText.length;
        const newEnd = newText.length;
        while (suffixLen < minLen - prefixLen &&
               oldText[oldEnd - suffixLen - 1] === newText[newEnd - suffixLen - 1]) {
            suffixLen++;
        }

        // Calculate splice parameters
        const pos = prefixLen;
        const del = oldEnd - prefixLen - suffixLen;
        const text = newText.substring(prefixLen, newEnd - suffixLen);

        // No change if nothing was deleted and nothing was inserted
        if (del === 0 && text.length === 0) {
            return null;
        }

        return { pos, del, text };
    },

    /**
     * Append a log message to the sync log display.
     * @param {string} message - The message to log
     * @param {string} source - Source of the message (server, left, right, error, shareable)
     */
    log(message, source = 'server') {
        const logDiv = document.getElementById('sync-log');
        if (!logDiv) return;
        const timestamp = new Date().toLocaleTimeString();
        const className = `log-${source}`;
        logDiv.innerHTML += `<span class="${className}">[${timestamp}] [${source.toUpperCase()}] ${message}</span>\n`;
        logDiv.scrollTop = logDiv.scrollHeight;
    },

    /**
     * Check if Webdis/Redis is reachable by sending a PING command.
     * @returns {Promise<boolean>} True if connected
     */
    async checkConnection() {
        try {
            const response = await fetch(`${this.WEBDIS_URL}/PING`);
            const data = await response.json();
            return data.PING === 'PONG' || data.PING === true || (Array.isArray(data.PING) && data.PING[1] === 'PONG');
        } catch (error) {
            return false;
        }
    },

    /**
     * Initialize a new Automerge document in Redis.
     * @param {string} docKey - The document key
     * @returns {Promise<boolean>} True if successful
     */
    async initializeDocument(docKey) {
        try {
            this.log(`Initializing document: ${docKey}`, 'server');

            // Create the document in Redis using AM.NEW
            const newResponse = await fetch(`${this.WEBDIS_URL}/AM.NEW/${docKey}`);
            const newData = await newResponse.json();
            this.log(`Document created in Redis: ${JSON.stringify(newData)}`, 'server');

            // Initialize the text field with empty string
            const initResponse = await fetch(`${this.WEBDIS_URL}/AM.PUTTEXT/${docKey}/text/`);
            const initData = await initResponse.json();
            this.log(`Text field initialized: ${JSON.stringify(initData)}`, 'server');

            return true;
        } catch (error) {
            this.log(`Error initializing document: ${error.message}`, 'error');
            return false;
        }
    },

    /**
     * Apply a splice operation to the server document.
     * @param {string} docKey - The document key
     * @param {Object} splice - The splice operation {pos, del, text}
     * @returns {Promise<boolean>} True if successful
     */
    async applySpliceToServer(docKey, splice) {
        try {
            const response = await fetch(`${this.WEBDIS_URL}/AM.SPLICETEXT/${docKey}/text/${splice.pos}/${splice.del}`, {
                method: 'PUT',
                headers: {
                    'Content-Type': 'text/plain'
                },
                body: splice.text
            });
            const data = await response.json();

            if (data['AM.SPLICETEXT'] && (data['AM.SPLICETEXT'] === 'OK' || data['AM.SPLICETEXT'][0] === true)) {
                return true;
            } else {
                this.log(`AM.SPLICETEXT error: ${JSON.stringify(data)}`, 'error');
                return false;
            }
        } catch (error) {
            this.log(`Error applying splice to server: ${error.message}`, 'error');
            console.error('Server splice error:', error);
            return false;
        }
    },

    /**
     * Set up WebSocket connection for real-time pub/sub synchronization.
     * @param {string} peerId - This peer's ID
     * @param {string} docKey - The document key to subscribe to
     * @param {Function} onMessage - Callback(channelName, messageData)
     * @param {Function} onKeyspace - Callback(command)
     * @returns {WebSocket} The WebSocket instance
     */
    setupWebSocket(peerId, docKey, onMessage, onKeyspace) {
        const channel = `changes:${docKey}`;
        const keyspaceChannel = `__keyspace@0__:${docKey}`;

        const ws = new WebSocket(`${this.WEBDIS_WS_URL}/.json`);

        ws.onopen = () => {
            this.log(`[${peerId}] WebSocket connected`, 'server');

            // Subscribe to the changes channel
            ws.send(JSON.stringify(['SUBSCRIBE', channel]));
            this.log(`[${peerId}] Subscribed to ${channel}`, 'server');

            // Subscribe to keyspace notifications
            ws.send(JSON.stringify(['SUBSCRIBE', keyspaceChannel]));
            this.log(`[${peerId}] Subscribed to ${keyspaceChannel}`, 'server');
        };

        ws.onmessage = (event) => {
            try {
                const response = JSON.parse(event.data);

                if (response.SUBSCRIBE || response.MESSAGE) {
                    const data = response.SUBSCRIBE || response.MESSAGE;
                    if (data[0] === 'subscribe') {
                        this.log(`[${peerId}] Subscription confirmed for ${data[1]}`, 'server');
                    } else if (data[0] === 'message') {
                        const channelName = data[1];
                        const messageData = data[2];

                        if (channelName.startsWith('__keyspace@')) {
                            // Keyspace notification
                            if (onKeyspace) onKeyspace(messageData);
                        } else {
                            // Regular message
                            if (onMessage) onMessage(channelName, messageData);
                        }
                    }
                }
            } catch (error) {
                this.log(`[${peerId}] WebSocket message error: ${error.message}`, 'error');
                console.error('WebSocket message error:', error, event.data);
            }
        };

        ws.onerror = (error) => {
            this.log(`[${peerId}] WebSocket error`, 'error');
            console.error(`[${peerId}] WebSocket error:`, error);
        };

        ws.onclose = (event) => {
            this.log(`[${peerId}] WebSocket disconnected (code: ${event.code})`, 'server');
        };

        return ws;
    }
};
```

**Step 2: Update existing global variables to reference EditorCore**

Replace the global constants at the top with references to EditorCore:

```javascript
// Use EditorCore constants
const WEBDIS_URL = EditorCore.WEBDIS_URL;
const WEBDIS_WS_URL = EditorCore.WEBDIS_WS_URL;
```

**Step 3: Update existing functions to use EditorCore where appropriate**

Update the `generatePeerId()` calls:

```javascript
// Old:
// let leftPeerId = generatePeerId();
// let rightPeerId = generatePeerId();

// New:
let leftPeerId = EditorCore.generatePeerId();
let rightPeerId = EditorCore.generatePeerId();
```

Update the `debounce()` calls:

```javascript
// Old:
// leftEditor.addEventListener('input', debounce(() => handleEdit('left'), 300));

// New:
leftEditor.addEventListener('input', EditorCore.debounce(() => handleEdit('left'), 300));
```

Update `calculateSplice()` calls in `handleEdit()`:

```javascript
// Old:
// const splice = calculateSplice(oldText, newText);

// New:
const splice = EditorCore.calculateSplice(oldText, newText);
```

**Step 4: Test that dual-pane mode still works**

Run:
```bash
docker compose up -d redis webdis
# Open demo/editor.html in browser
# Test creating document and syncing between panes
```

Expected: Dual-pane editor functions identically to before refactoring

**Step 5: Commit**

```bash
git add demo/editor-app.js
git commit -m "refactor: extract EditorCore namespace with shared primitives"
```

---

## Task 3: Add Tab UI Structure

**Files:**
- Modify: `demo/editor.html`
- Modify: `demo/editor-styles.css`

**Step 1: Add tab selector to HTML**

In `demo/editor.html`, add the tab selector before the existing controls div:

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Redis-Automerge Collaborative Editor Demo</title>
    <link rel="stylesheet" href="editor-styles.css">
    <script src="https://cdn.jsdelivr.net/npm/@automerge/automerge/dist/index.js"></script>
</head>
<body>
    <h1>Redis-Automerge Collaborative Editor</h1>

    <!-- Tab Selector -->
    <div id="mode-selector" class="tab-container">
        <button class="tab-btn active" data-mode="dual" onclick="switchMode('dual')">
            Dual Editor Mode
        </button>
        <button class="tab-btn" data-mode="shareable" onclick="switchMode('shareable')">
            Shareable Link Mode
        </button>
    </div>

    <!-- Dual Editor Mode Panel (existing content) -->
    <div id="dual-mode" class="mode-panel active">
        <!-- Keep all existing editor HTML content here -->
    </div>

    <!-- Shareable Editor Mode Panel (new) -->
    <div id="shareable-mode" class="mode-panel hidden">
        <div class="controls">
            <div class="status-bar">
                <span>Connection: </span>
                <span id="connection-status-shareable" class="status-disconnected">Checking...</span>
                <span style="margin-left: 20px;">Sync: </span>
                <span id="sync-status-shareable">Not connected</span>
            </div>
        </div>

        <div id="room-selector" class="room-selector">
            <h3>Join or Create Room</h3>

            <div id="room-list-container">
                <button onclick="ShareableMode.refreshRoomList()" class="action-button">
                    Refresh Room List
                </button>
                <div id="room-list" class="room-list">
                    <!-- Dynamically populated -->
                </div>
            </div>

            <div class="create-room-controls">
                <h4>Create New Room</h4>
                <button onclick="ShareableMode.createRandomRoom()" class="action-button">
                    Create Random Room
                </button>
                <div style="margin-top: 10px;">
                    <input id="custom-room-name" type="text" placeholder="Or enter custom room name"
                           style="width: 300px; padding: 5px;">
                    <button onclick="ShareableMode.createCustomRoom()" class="action-button">
                        Create Custom Room
                    </button>
                </div>
            </div>
        </div>

        <div id="editor-container-shareable" class="editor-container hidden">
            <div id="room-info" class="room-info">
                <span>Room: <strong id="current-room-name"></strong></span>
                <button onclick="ShareableMode.copyShareableLink()" class="action-button">
                    Copy Link
                </button>
                <button onclick="ShareableMode.disconnect()" class="action-button">
                    Disconnect
                </button>
            </div>

            <div class="editor-section">
                <div class="editor-header">
                    <h3>Shared Editor</h3>
                    <span id="peer-id-shareable"></span>
                </div>
                <textarea id="editor-shareable" class="editor" placeholder="Start typing..."></textarea>
                <div id="info-shareable" class="doc-info"></div>
                <div id="doc-version-shareable" class="doc-version"></div>
            </div>
        </div>
    </div>

    <!-- Keep existing sync log -->
    <div class="sync-log-section">
        <div class="log-controls">
            <h3>Sync Log</h3>
            <button onclick="clearLog()" class="log-button">Clear</button>
            <button onclick="selectAllLog()" class="log-button">Select All</button>
        </div>
        <pre id="sync-log" class="sync-log"></pre>
    </div>

    <script src="editor-app.js"></script>
</body>
</html>
```

**Step 2: Add CSS for tabs and shareable mode**

In `demo/editor-styles.css`, add:

```css
/* Tab selector */
.tab-container {
    display: flex;
    gap: 0;
    margin-bottom: 20px;
    border-bottom: 2px solid #ddd;
}

.tab-btn {
    padding: 12px 24px;
    background: #f5f5f5;
    border: none;
    border-bottom: 3px solid transparent;
    cursor: pointer;
    font-size: 16px;
    transition: all 0.3s;
}

.tab-btn:hover {
    background: #e8e8e8;
}

.tab-btn.active {
    background: white;
    border-bottom-color: #2196F3;
    font-weight: bold;
}

/* Mode panels */
.mode-panel {
    display: block;
}

.mode-panel.hidden {
    display: none;
}

/* Room selector */
.room-selector {
    padding: 20px;
    background: #f9f9f9;
    border-radius: 8px;
    margin-bottom: 20px;
}

.room-list {
    margin: 15px 0;
    max-height: 300px;
    overflow-y: auto;
    border: 1px solid #ddd;
    border-radius: 4px;
    background: white;
}

.room-item {
    padding: 12px;
    border-bottom: 1px solid #eee;
    display: flex;
    justify-content: space-between;
    align-items: center;
    cursor: pointer;
    transition: background 0.2s;
}

.room-item:hover {
    background: #f0f0f0;
}

.room-item:last-child {
    border-bottom: none;
}

.room-name {
    font-weight: bold;
    color: #333;
}

.room-users {
    color: #666;
    font-size: 14px;
}

.room-users.active {
    color: #4CAF50;
    font-weight: bold;
}

.create-room-controls {
    margin-top: 20px;
    padding-top: 20px;
    border-top: 1px solid #ddd;
}

/* Room info bar */
.room-info {
    padding: 12px;
    background: #e3f2fd;
    border-radius: 4px;
    margin-bottom: 15px;
    display: flex;
    align-items: center;
    gap: 15px;
}

.editor-container {
    display: block;
}

.editor-container.hidden {
    display: none;
}

#connection-status-shareable {
    font-weight: bold;
}
```

**Step 3: Add tab switching function to editor-app.js**

At the bottom of `demo/editor-app.js`, add:

```javascript
/**
 * Switch between dual and shareable modes
 * @param {string} mode - 'dual' or 'shareable'
 */
function switchMode(mode) {
    // Update tab buttons
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.mode === mode);
    });

    // Update panels
    document.getElementById('dual-mode').classList.toggle('active', mode === 'dual');
    document.getElementById('dual-mode').classList.toggle('hidden', mode !== 'dual');
    document.getElementById('shareable-mode').classList.toggle('active', mode === 'shareable');
    document.getElementById('shareable-mode').classList.toggle('hidden', mode !== 'shareable');

    EditorCore.log(`Switched to ${mode} mode`, 'server');
}

// Make it globally available
window.switchMode = switchMode;
```

**Step 4: Test tab switching**

Run:
```bash
# Open demo/editor.html in browser
# Click between tabs
```

Expected: Tabs switch visual appearance, panels show/hide correctly

**Step 5: Commit**

```bash
git add demo/editor.html demo/editor-styles.css demo/editor-app.js
git commit -m "feat: add tab UI for dual and shareable modes"
```

---

## Task 4: Implement ShareableMode Room Management

**Files:**
- Modify: `demo/editor-app.js` (add ShareableMode namespace)

**Step 1: Add ShareableMode namespace**

In `demo/editor-app.js`, after EditorCore and before the DualPaneMode code, add:

```javascript
// ============================================================================
// ShareableMode - Single editor with room management
// ============================================================================
const ShareableMode = {
    doc: null,
    peerId: null,
    socket: null,
    roomListSocket: null,
    prevText: '',
    currentRoom: null,
    roomList: [],

    /**
     * Initialize shareable mode
     */
    async initialize() {
        this.peerId = EditorCore.generatePeerId();
        document.getElementById('peer-id-shareable').textContent = `Peer ID: ${this.peerId.slice(0, 8)}`;

        // Check for room in URL
        const urlRoom = this.parseUrlRoom();
        if (urlRoom) {
            EditorCore.log(`Room from URL: ${urlRoom}`, 'shareable');
            // Auto-select shareable tab
            switchMode('shareable');
            // Show room but don't auto-join (user clicks to join)
            await this.refreshRoomList();
        } else {
            await this.refreshRoomList();
        }

        // Subscribe to room list updates
        this.subscribeToRoomEvents();
    },

    /**
     * Parse room name from URL parameters
     * @returns {string|null} Room name or null
     */
    parseUrlRoom() {
        const params = new URLSearchParams(window.location.search);
        return params.get('room');
    },

    /**
     * Generate a random 8-character room ID
     * @returns {string} Random room ID
     */
    generateRoomId() {
        return Math.random().toString(36).substring(2, 10);
    },

    /**
     * Create a new room with random ID
     */
    async createRandomRoom() {
        const roomId = this.generateRoomId();
        await this.createRoom(roomId);
    },

    /**
     * Create a new room with custom name
     */
    async createCustomRoom() {
        const input = document.getElementById('custom-room-name');
        const roomName = input.value.trim();

        if (!roomName) {
            alert('Please enter a room name');
            return;
        }

        if (!this.validateRoomName(roomName)) {
            alert('Room name must be 1-50 characters (letters, numbers, dash, underscore only)');
            return;
        }

        input.value = '';
        await this.createRoom(roomName);
    },

    /**
     * Validate room name format
     * @param {string} name - Room name to validate
     * @returns {boolean} True if valid
     */
    validateRoomName(name) {
        return /^[a-zA-Z0-9_-]{1,50}$/.test(name);
    },

    /**
     * Create a new room
     * @param {string} roomName - The room name
     */
    async createRoom(roomName) {
        const docKey = `am:room:${roomName}`;

        EditorCore.log(`Creating room: ${roomName}`, 'shareable');

        // Check if room already exists
        const exists = await this.checkRoomExists(docKey);
        if (exists) {
            const join = confirm(`Room "${roomName}" already exists. Join instead?`);
            if (join) {
                await this.joinRoom(roomName);
            }
            return;
        }

        // Create the room
        const success = await EditorCore.initializeDocument(docKey);
        if (success) {
            EditorCore.log(`Room created: ${roomName}`, 'shareable');
            await this.joinRoom(roomName);
        } else {
            alert('Failed to create room. Please try again.');
        }
    },

    /**
     * Check if a room exists in Redis
     * @param {string} docKey - The document key
     * @returns {Promise<boolean>} True if exists
     */
    async checkRoomExists(docKey) {
        try {
            const response = await fetch(`${EditorCore.WEBDIS_URL}/EXISTS/${docKey}`);
            const data = await response.json();
            return data.EXISTS === 1 || data.EXISTS === true;
        } catch (error) {
            EditorCore.log(`Error checking room existence: ${error.message}`, 'error');
            return false;
        }
    },

    /**
     * Join an existing room
     * @param {string} roomName - The room name
     */
    async joinRoom(roomName) {
        const docKey = `am:room:${roomName}`;

        EditorCore.log(`Joining room: ${roomName}`, 'shareable');

        // Initialize local Automerge document
        const baseDoc = Automerge.init();
        this.doc = Automerge.change(baseDoc, doc => {
            doc.text = '';
        });
        this.prevText = '';

        // Set up WebSocket for changes
        this.socket = EditorCore.setupWebSocket(
            this.peerId,
            docKey,
            (channel, messageData) => this.handleIncomingMessage(messageData),
            (command) => this.handleKeyspaceNotification(docKey, command)
        );

        this.currentRoom = roomName;

        // Update UI
        document.getElementById('room-selector').classList.add('hidden');
        document.getElementById('editor-container-shareable').classList.remove('hidden');
        document.getElementById('current-room-name').textContent = roomName;
        document.getElementById('sync-status-shareable').textContent = 'Syncing ✓';

        // Set up editor event listener
        const editor = document.getElementById('editor-shareable');
        editor.addEventListener('input', EditorCore.debounce(() => this.handleEdit(), 300));

        // Fetch current text from server
        await this.fetchServerText(docKey);

        EditorCore.log(`Connected to room: ${roomName}`, 'shareable');

        // Update URL without reloading
        const url = new URL(window.location);
        url.searchParams.set('room', roomName);
        window.history.pushState({}, '', url);
    },

    /**
     * Fetch current text from server and update local doc
     * @param {string} docKey - The document key
     */
    async fetchServerText(docKey) {
        try {
            const response = await fetch(`${EditorCore.WEBDIS_URL}/AM.GETTEXT/${docKey}/text`);
            const data = await response.json();

            if (data && data['AM.GETTEXT']) {
                const serverText = data['AM.GETTEXT'];
                this.doc = Automerge.change(this.doc, doc => {
                    doc.text = serverText;
                });
                this.updateEditor();
                this.updateDocInfo();
            }
        } catch (error) {
            EditorCore.log(`Error fetching server text: ${error.message}`, 'error');
        }
    },

    /**
     * Handle incoming change messages from Redis pub/sub
     * @param {string} messageData - Base64-encoded change bytes
     */
    handleIncomingMessage(messageData) {
        try {
            const changeBytes = Uint8Array.from(atob(messageData), c => c.charCodeAt(0));
            EditorCore.log('Received change from server', 'shareable');

            const [newDoc] = Automerge.applyChanges(this.doc, [changeBytes]);
            this.doc = newDoc;

            this.updateEditor();
            this.updateDocInfo();
        } catch (error) {
            EditorCore.log(`Error handling message: ${error.message}`, 'error');
        }
    },

    /**
     * Handle keyspace notification events
     * @param {string} docKey - The document key
     * @param {string} command - The Redis command that triggered the notification
     */
    async handleKeyspaceNotification(docKey, command) {
        EditorCore.log(`Keyspace notification: ${command} on ${docKey}`, 'shareable');
        await this.fetchServerText(docKey);
    },

    /**
     * Handle local editor changes
     */
    async handleEdit() {
        if (!this.currentRoom) return;

        const docKey = `am:room:${this.currentRoom}`;
        const textarea = document.getElementById('editor-shareable');
        const newText = textarea.value;

        // Calculate the minimal splice operation
        const splice = EditorCore.calculateSplice(this.prevText, newText);

        if (!splice) {
            return;
        }

        // Update local document
        this.doc = Automerge.change(this.doc, d => {
            d.text = newText;
        });
        this.prevText = newText;

        EditorCore.log(`Local edit: pos=${splice.pos}, del=${splice.del}, insert="${splice.text.substring(0, 20)}${splice.text.length > 20 ? '...' : ''}"`, 'shareable');

        // Apply to server
        await EditorCore.applySpliceToServer(docKey, splice);
        this.updateDocInfo();
    },

    /**
     * Update the editor textarea
     */
    updateEditor() {
        const textarea = document.getElementById('editor-shareable');
        const oldText = textarea.value;
        const newText = this.doc.text || '';
        const cursorPos = textarea.selectionStart;

        if (newText !== oldText) {
            const splice = EditorCore.calculateSplice(oldText, newText);
            textarea.value = newText;

            if (splice) {
                let newCursorPos = cursorPos;
                if (cursorPos >= splice.pos) {
                    const netChange = splice.text.length - splice.del;
                    if (cursorPos <= splice.pos + splice.del) {
                        newCursorPos = splice.pos + splice.text.length;
                    } else {
                        newCursorPos = cursorPos + netChange;
                    }
                }
                newCursorPos = Math.max(0, Math.min(newCursorPos, newText.length));
                textarea.setSelectionRange(newCursorPos, newCursorPos);
            }

            this.prevText = newText;
        }
    },

    /**
     * Update document info panel
     */
    updateDocInfo() {
        if (!this.doc) return;

        const infoDiv = document.getElementById('info-shareable');
        const history = Automerge.getHistory(this.doc);

        infoDiv.innerHTML = `
            <div>Characters: ${(this.doc.text || '').length}</div>
            <div>Changes: ${history.length}</div>
            <div>Peer: ${this.peerId.slice(0, 8)}</div>
        `;

        document.getElementById('doc-version-shareable').textContent = `v${history.length}`;
    },

    /**
     * Disconnect from current room
     */
    disconnect() {
        if (this.socket) {
            this.socket.close();
            this.socket = null;
        }

        this.doc = null;
        this.currentRoom = null;
        this.prevText = '';

        // Update UI
        document.getElementById('room-selector').classList.remove('hidden');
        document.getElementById('editor-container-shareable').classList.add('hidden');
        document.getElementById('editor-shareable').value = '';
        document.getElementById('sync-status-shareable').textContent = 'Not connected';

        // Remove room from URL
        const url = new URL(window.location);
        url.searchParams.delete('room');
        window.history.pushState({}, '', url);

        EditorCore.log('Disconnected from room', 'shareable');
    },

    /**
     * Copy shareable link to clipboard
     */
    async copyShareableLink() {
        if (!this.currentRoom) return;

        const url = new URL(window.location);
        url.searchParams.set('room', this.currentRoom);
        const link = url.toString();

        try {
            await navigator.clipboard.writeText(link);
            alert(`Link copied: ${link}`);
            EditorCore.log('Shareable link copied to clipboard', 'shareable');
        } catch (error) {
            prompt('Copy this link:', link);
        }
    },

    /**
     * Refresh the room list
     */
    async refreshRoomList() {
        EditorCore.log('Refreshing room list', 'shareable');

        try {
            // Use KEYS to find all rooms (for demo; use SCAN in production)
            const response = await fetch(`${EditorCore.WEBDIS_URL}/KEYS/am:room:*`);
            const data = await response.json();

            const rooms = (data.KEYS || []).map(key => {
                // Extract room name from key
                return key.replace('am:room:', '');
            });

            // Get active user counts
            const roomsWithUsers = await Promise.all(
                rooms.map(async (roomName) => {
                    const channel = `changes:am:room:${roomName}`;
                    const numsubResponse = await fetch(`${EditorCore.WEBDIS_URL}/PUBSUB/NUMSUB/${channel}`);
                    const numsubData = await numsubResponse.json();

                    let activeUsers = 0;
                    if (numsubData['PUBSUB NUMSUB'] && Array.isArray(numsubData['PUBSUB NUMSUB'])) {
                        activeUsers = numsubData['PUBSUB NUMSUB'][1] || 0;
                    }

                    return { roomName, activeUsers };
                })
            );

            this.roomList = roomsWithUsers;
            this.updateRoomListUI();

        } catch (error) {
            EditorCore.log(`Error refreshing room list: ${error.message}`, 'error');
        }
    },

    /**
     * Update the room list UI
     */
    updateRoomListUI() {
        const listDiv = document.getElementById('room-list');

        if (this.roomList.length === 0) {
            listDiv.innerHTML = '<div style="padding: 20px; text-align: center; color: #999;">No rooms available. Create one to get started!</div>';
            return;
        }

        listDiv.innerHTML = this.roomList.map(({ roomName, activeUsers }) => {
            const userClass = activeUsers > 0 ? 'active' : '';
            const userText = activeUsers === 1 ? '1 user' : `${activeUsers} users`;
            return `
                <div class="room-item" onclick="ShareableMode.joinRoom('${roomName}')">
                    <span class="room-name">${roomName}</span>
                    <span class="room-users ${userClass}">${userText}</span>
                </div>
            `;
        }).join('');
    },

    /**
     * Subscribe to room creation/deletion events
     */
    subscribeToRoomEvents() {
        const pattern = '__keyspace@0__:am:room:*';

        this.roomListSocket = new WebSocket(`${EditorCore.WEBDIS_WS_URL}/.json`);

        this.roomListSocket.onopen = () => {
            EditorCore.log('Room list WebSocket connected', 'shareable');
            this.roomListSocket.send(JSON.stringify(['PSUBSCRIBE', pattern]));
        };

        this.roomListSocket.onmessage = async (event) => {
            try {
                const response = JSON.parse(event.data);

                if (response.PSUBSCRIBE || response.PMESSAGE) {
                    const data = response.PSUBSCRIBE || response.PMESSAGE;

                    if (data[0] === 'psubscribe') {
                        EditorCore.log(`Subscribed to pattern: ${data[1]}`, 'shareable');
                    } else if (data[0] === 'pmessage') {
                        const command = data[3];
                        EditorCore.log(`Room event detected: ${command}`, 'shareable');
                        // Refresh room list when rooms are created or deleted
                        if (command === 'am.new' || command === 'del') {
                            await this.refreshRoomList();
                        }
                    }
                }
            } catch (error) {
                EditorCore.log(`Room list WebSocket error: ${error.message}`, 'error');
            }
        };

        this.roomListSocket.onerror = (error) => {
            EditorCore.log('Room list WebSocket error', 'error');
        };

        this.roomListSocket.onclose = () => {
            EditorCore.log('Room list WebSocket closed', 'shareable');
        };
    }
};

// Make ShareableMode globally available
window.ShareableMode = ShareableMode;
```

**Step 2: Initialize ShareableMode on page load**

Update the window load event listener in `demo/editor-app.js`:

```javascript
window.addEventListener('load', async () => {
    log('Collaborative editor loaded', 'server');

    // Check if Automerge loaded
    if (typeof Automerge === 'undefined') {
        log('ERROR: Automerge library failed to load!', 'error');
    } else {
        log('Automerge library ready', 'server');
    }

    // Check connection
    const connected = await EditorCore.checkConnection();
    updateConnectionStatus(connected);

    // Initialize ShareableMode
    await ShareableMode.initialize();

    // Set peer IDs for dual mode
    document.getElementById('peer-id-left').textContent = `Peer ID: ${leftPeerId.slice(0, 8)}`;
    document.getElementById('peer-id-right').textContent = `Peer ID: ${rightPeerId.slice(0, 8)}`;

    // ... rest of existing code
});
```

**Step 3: Test room creation and joining**

Run:
```bash
docker compose up -d redis webdis
# Open demo/editor.html in browser
# Click "Shareable Link Mode" tab
# Click "Create Random Room"
# Verify room is created and editor appears
```

Expected: Room created, editor loads, can type text

**Step 4: Test in multiple browser tabs**

Run:
```bash
# Keep first tab open with editor
# Copy shareable link
# Open in new tab
# Join same room from list or URL
# Type in one tab, verify sync to other tab
```

Expected: Changes sync between tabs in real-time

**Step 5: Commit**

```bash
git add demo/editor-app.js
git commit -m "feat: implement ShareableMode with room management and real-time sync"
```

---

## Task 5: Add Documentation

**Files:**
- Create: `demo/SHAREABLE_EDITOR.md`

**Step 1: Create documentation file**

```markdown
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
```

**Step 2: Update main README**

In `demo/README.md`, add reference to shareable editor:

```markdown
## Demos

- **[Collaborative Editor](COLLABORATIVE_EDITOR.md)**: Dual-pane editor demonstrating real-time sync
- **[Shareable Link Editor](SHAREABLE_EDITOR.md)**: Single-pane editor with shareable rooms (NEW)
```

**Step 3: Commit**

```bash
git add demo/SHAREABLE_EDITOR.md demo/README.md
git commit -m "docs: add shareable editor documentation"
```

---

## Task 6: Add Integration Tests

**Files:**
- Modify: `scripts/test-module.sh` (add shareable mode tests)

**Step 1: Add test functions for shareable mode**

In `scripts/test-module.sh`, after existing tests, add:

```bash
# Test: Create room with specific key
test_create_room() {
    echo "Test: Create room"

    ROOM_KEY="am:room:test-room-$$"

    # Create room
    RESULT=$(redis-cli AM.NEW "$ROOM_KEY")
    assert_equals "OK" "$RESULT" "Room creation failed"

    # Initialize text field
    RESULT=$(redis-cli AM.PUTTEXT "$ROOM_KEY" text "")
    assert_equals "OK" "$RESULT" "Text initialization failed"

    # Verify room exists
    RESULT=$(redis-cli EXISTS "$ROOM_KEY")
    assert_equals "1" "$RESULT" "Room doesn't exist"

    echo "✓ Room creation works"
}

# Test: Join existing room
test_join_room() {
    echo "Test: Join existing room"

    ROOM_KEY="am:room:join-test-$$"

    # Create room
    redis-cli AM.NEW "$ROOM_KEY" > /dev/null
    redis-cli AM.PUTTEXT "$ROOM_KEY" text "Hello" > /dev/null

    # Simulate second client fetching text
    RESULT=$(redis-cli AM.GETTEXT "$ROOM_KEY" text)
    assert_equals "Hello" "$RESULT" "Failed to fetch room text"

    echo "✓ Room joining works"
}

# Test: Room list via KEYS
test_room_list() {
    echo "Test: Room list"

    # Create multiple rooms
    redis-cli AM.NEW "am:room:list-test-1" > /dev/null
    redis-cli AM.NEW "am:room:list-test-2" > /dev/null
    redis-cli AM.NEW "am:room:list-test-3" > /dev/null

    # Fetch room list
    RESULT=$(redis-cli KEYS "am:room:list-test-*" | wc -l)

    if [ "$RESULT" -ge 3 ]; then
        echo "✓ Room list works (found $RESULT rooms)"
    else
        echo "✗ Room list failed (found $RESULT rooms, expected >= 3)"
        exit 1
    fi
}

# Test: Keyspace notifications
test_keyspace_notifications() {
    echo "Test: Keyspace notifications"

    # Check if keyspace notifications enabled
    RESULT=$(redis-cli CONFIG GET notify-keyspace-events | tail -1)

    if echo "$RESULT" | grep -q "K"; then
        echo "✓ Keyspace notifications enabled: $RESULT"
    else
        echo "✗ Keyspace notifications not enabled: $RESULT"
        exit 1
    fi
}

# Run shareable mode tests
echo ""
echo "=== Shareable Mode Tests ==="
test_keyspace_notifications
test_create_room
test_join_room
test_room_list
```

**Step 2: Run integration tests**

Run:
```bash
docker compose run --build --rm test
```

Expected: All tests pass including new shareable mode tests

**Step 3: Commit**

```bash
git add scripts/test-module.sh
git commit -m "test: add integration tests for shareable editor mode"
```

---

## Task 7: Final Verification

**Files:**
- All files in demo/

**Step 1: Full end-to-end test**

Run:
```bash
# Start services
docker compose up -d redis webdis

# Open demo in browser
# Test dual-pane mode still works
# Test shareable mode:
#   - Create random room
#   - Create custom room
#   - Join room from list
#   - Copy and open shareable link in new tab
#   - Verify real-time sync between tabs
#   - Test room list refresh
#   - Verify active user counts
```

Expected: All features work end-to-end

**Step 2: Run all tests**

Run:
```bash
# Unit tests
cargo test --verbose --manifest-path redis-automerge/Cargo.toml

# Integration tests
docker compose run --build --rm test
docker compose down
```

Expected: All tests pass

**Step 3: Final commit**

```bash
git add .
git commit -m "feat: complete shareable editor implementation with room management

- EditorCore namespace with shared primitives
- ShareableMode with room CRUD and real-time list
- Tab-based UI for dual and shareable modes
- Redis keyspace notifications for room tracking
- PUBSUB NUMSUB for active user counts
- URL parameter support for direct room joining
- Comprehensive documentation and tests"
```

---

## Summary

This plan implements a shareable link collaborative editor with:

1. **Module Refactoring**: EditorCore namespace extracts shared primitives
2. **Tab UI**: Clean switching between dual-pane and shareable modes
3. **Room Management**: Create random/custom rooms, real-time room list
4. **Real-Time Sync**: WebSocket-based pub/sub with cursor preservation
5. **Shareable Links**: URL parameters for direct room access
6. **Active Users**: Track connected users via PUBSUB NUMSUB
7. **Keyspace Notifications**: Real-time room list updates
8. **Documentation**: Comprehensive guides and usage examples
9. **Tests**: Integration tests for room operations

**Key Architectural Decisions**:
- Module pattern (not ES6 modules) for browser compatibility
- Redis keyspace notifications for real-time room discovery
- PUBSUB NUMSUB for active user tracking (no separate presence management)
- URL parameters for shareable links (no hash routing)
- KEYS for room list (acceptable for demo, document SCAN for production)

**Testing Strategy**:
- Unit tests verify core Rust module unchanged
- Integration tests verify room operations
- Manual browser tests verify end-to-end UX

**Next Steps After Implementation**:
1. Test with multiple browsers simultaneously
2. Verify room list updates in real-time
3. Test edge cases (duplicate names, disconnections, etc.)
4. Consider production improvements (SCAN, rate limiting, auth)
