// ============================================================================
// EditorCore - Shared primitives used by both modes
// ============================================================================
const EditorCore = {
    WEBDIS_URL: `${window.location.protocol}//${window.location.host}/api`,
    WEBDIS_WS_URL: `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/api`,

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
            let response;
            if (splice.text === '') {
                // Webdis doesn't send empty PUT body as an argument to Redis
                // Use GET with trailing slash for empty string (deletions)
                const url = `${this.WEBDIS_URL}/AM.SPLICETEXT/${docKey}/text/${splice.pos}/${splice.del}/`;
                response = await fetch(url);
            } else {
                // For insertions/replacements, use PUT with text in body
                const url = `${this.WEBDIS_URL}/AM.SPLICETEXT/${docKey}/text/${splice.pos}/${splice.del}`;
                response = await fetch(url, {
                    method: 'PUT',
                    headers: {
                        'Content-Type': 'text/plain'
                    },
                    body: splice.text
                });
            }

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
    roomPeers: {}, // Track peers by room: { roomName: Set of peerIds }
    roomActivity: {}, // Track last activity timestamp by room: { roomName: timestamp }
    _currentHandler: null,
    _refreshDebounce: null,
    refreshInterval: null,
    heartbeatInterval: null,
    isLocallyEditing: false, // Track if we're actively making changes

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
            if (typeof switchMode !== 'undefined') {
                switchMode('shareable');
            }
            // Auto-join the room from URL
            await this.refreshRoomList();
            await this.joinRoom(urlRoom);
        } else {
            await this.refreshRoomList();
        }

        // Subscribe to room list updates
        this.subscribeToRoomEvents();

        // Periodically refresh room list to update user counts (lightweight operation)
        this.startRoomListRefresh();
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
        // Check if already in this room
        if (this.currentRoom === roomName) {
            EditorCore.log(`Already connected to room: ${roomName}`, 'shareable');
            return;
        }

        // If already in another room, disconnect first
        if (this.currentRoom) {
            EditorCore.log(`Leaving room: ${this.currentRoom}`, 'shareable');
            if (this.socket) {
                this.socket.close();
                this.socket = null;
            }
        }

        const docKey = `am:room:${roomName}`;

        EditorCore.log(`Joining room: ${roomName}`, 'shareable');

        // Load the document from Redis to ensure all clients share the same document history
        // Use .raw endpoint because Webdis .json endpoint can't handle binary data
        try {
            const response = await fetch(`${EditorCore.WEBDIS_URL}/AM.SAVE/${docKey}.raw`);

            if (response.ok) {
                // Get raw binary data as ArrayBuffer
                const arrayBuffer = await response.arrayBuffer();
                const docBytes = new Uint8Array(arrayBuffer);

                // Webdis .raw returns Redis protocol format: $NNN\r\n<data>\r\n
                // Parse the bulk string header to get exact byte count
                const decoder = new TextDecoder('utf-8');
                let headerEnd = 0;
                for (let i = 0; i < docBytes.length - 1; i++) {
                    if (docBytes[i] === 0x0d && docBytes[i + 1] === 0x0a) {
                        headerEnd = i;
                        break;
                    }
                }

                // Extract byte count from header: $518\r\n -> "518"
                const headerText = decoder.decode(docBytes.slice(1, headerEnd));
                const byteCount = parseInt(headerText, 10);
                const dataStart = headerEnd + 2; // Skip past \r\n

                // Extract exactly byteCount bytes (ignore trailing \r\n)
                const actualDocBytes = docBytes.slice(dataStart, dataStart + byteCount);

                this.doc = Automerge.load(actualDocBytes);

                // Convert Automerge Text object to plain string
                const textValue = this.doc.text ? this.doc.text.toString() : '';
                this.prevText = textValue;
                EditorCore.log(`Loaded document from server (${actualDocBytes.length} bytes)`, 'shareable');
            } else {
                console.error('[ShareableMode] Failed to load document, status:', response.status);
                EditorCore.log('Failed to load document from server', 'error');
                const baseDoc = Automerge.init();
                this.doc = Automerge.change(baseDoc, doc => {
                    doc.text = '';
                });
                this.prevText = '';
            }
        } catch (error) {
            console.error('[ShareableMode] Error loading document:', error);
            EditorCore.log(`Error loading document: ${error.message}`, 'error');
            const baseDoc = Automerge.init();
            this.doc = Automerge.change(baseDoc, doc => {
                doc.text = '';
            });
            this.prevText = '';
        }

        // Set up WebSocket for changes
        this.socket = EditorCore.setupWebSocket(
            this.peerId,
            docKey,
            (channel, messageData) => this.handleIncomingMessage(messageData),
            (command) => this.handleKeyspaceNotification(docKey, command)
        );

        this.currentRoom = roomName;

        // Announce presence immediately
        await this.announcePresence(roomName);

        // Start heartbeat to maintain presence
        this.startHeartbeat(roomName);

        // Update UI
        document.getElementById('current-room-name').textContent = roomName;
        document.getElementById('sync-status-shareable-editor').textContent = 'Syncing ✓';

        // Set up editor event listener (remove old listener first to avoid duplicates)
        const editor = document.getElementById('editor-shareable');
        const newHandler = EditorCore.debounce(() => this.handleEdit(), 300);
        editor.removeEventListener('input', this._currentHandler);
        this._currentHandler = newHandler;
        editor.addEventListener('input', newHandler);

        // Update editor when it loses focus (to sync any pending changes)
        editor.addEventListener('blur', () => {
            EditorCore.log('Editor lost focus - syncing any pending changes', 'shareable');
            this.updateEditor();
        });

        // Update the editor with loaded content
        const textarea = document.getElementById('editor-shareable');
        // Convert Automerge Text object to plain string
        const textToDisplay = this.doc.text ? this.doc.text.toString() : '';
        textarea.value = textToDisplay;
        this.prevText = textToDisplay;

        EditorCore.log(`Connected to room: ${roomName}`, 'shareable');

        // Update URL without reloading
        const url = new URL(window.location);
        url.searchParams.set('room', roomName);
        window.history.pushState({}, '', url);

        // Load and display history
        await this.loadHistory(docKey);

        // Trigger immediate room list refresh to update user count
        await this.triggerRoomListRefresh();
    },

    /**
     * Load change history from AM.CHANGES and display in sidebar
     * @param {string} docKey - The document key
     */
    async loadHistory(docKey) {
        try {
            // Fetch all changes using AM.CHANGES with .raw endpoint
            const response = await fetch(`${EditorCore.WEBDIS_URL}/AM.CHANGES/${docKey}.raw`);

            if (!response.ok) {
                EditorCore.log('Failed to load history', 'error');
                return;
            }

            // Get raw binary data as ArrayBuffer
            const arrayBuffer = await response.arrayBuffer();
            const data = new Uint8Array(arrayBuffer);

            // Parse Redis protocol array response
            // Format: *N\r\n$len1\r\n<data1>\r\n$len2\r\n<data2>\r\n...
            const changes = this.parseRedisArray(data);

            if (changes.length === 0) {
                document.getElementById('history-list').innerHTML =
                    '<div style="padding: 20px; text-align: center; color: #999;">No history yet</div>';
                return;
            }

            // Apply changes to a temporary document to get history with context
            let tempDoc = Automerge.init();
            const historyItems = [];
            let prevText = '';

            for (const changeBytes of changes) {
                try {
                    const [newDoc] = Automerge.applyChanges(tempDoc, [changeBytes]);
                    const newText = newDoc.text ? newDoc.text.toString() : '';

                    // Calculate what changed
                    const changeDetails = this.calculateChangeDetails(prevText, newText);

                    tempDoc = newDoc;
                    prevText = newText;

                    // Get the latest history item
                    const history = Automerge.getHistory(tempDoc);
                    const item = history[history.length - 1];

                    historyItems.push({
                        index: history.length,
                        timestamp: item.change.time || Date.now(),
                        actor: item.change.actor || 'unknown',
                        message: item.change.message || 'Document change',
                        changeDetails: changeDetails
                    });
                } catch (error) {
                    console.error('Error processing change:', error);
                }
            }

            // Display history (newest first - reverse the array)
            this.displayHistory(historyItems.reverse());

            EditorCore.log(`Loaded ${changes.length} changes from history`, 'shareable');
        } catch (error) {
            EditorCore.log(`Error loading history: ${error.message}`, 'error');
            console.error('Load history error:', error);
        }
    },

    /**
     * Calculate detailed change information from text diff
     * @param {string} oldText - Previous text
     * @param {string} newText - New text
     * @returns {Object} Change details with type and description
     */
    calculateChangeDetails(oldText, newText) {
        if (oldText === '' && newText !== '') {
            // Initial content
            const preview = newText.length > 50 ? newText.substring(0, 50) + '...' : newText;
            return {
                type: 'insert',
                description: `Added initial content: "${preview}"`,
                added: newText.length,
                removed: 0
            };
        }

        if (oldText === newText) {
            return {
                type: 'none',
                description: 'No text change',
                added: 0,
                removed: 0
            };
        }

        // Calculate the splice operation
        const splice = EditorCore.calculateSplice(oldText, newText);
        if (!splice) {
            return {
                type: 'none',
                description: 'No text change',
                added: 0,
                removed: 0
            };
        }

        const deletedText = oldText.substring(splice.pos, splice.pos + splice.del);
        const insertedText = splice.text;

        if (splice.del > 0 && insertedText.length > 0) {
            // Replacement
            const delPreview = deletedText.length > 30 ? deletedText.substring(0, 30) + '...' : deletedText;
            const insPreview = insertedText.length > 30 ? insertedText.substring(0, 30) + '...' : insertedText;
            return {
                type: 'replace',
                description: `Replaced "${delPreview}" with "${insPreview}"`,
                added: insertedText.length,
                removed: deletedText.length,
                position: splice.pos
            };
        } else if (splice.del > 0) {
            // Deletion only
            const preview = deletedText.length > 50 ? deletedText.substring(0, 50) + '...' : deletedText;
            return {
                type: 'delete',
                description: `Deleted "${preview}"`,
                added: 0,
                removed: deletedText.length,
                position: splice.pos
            };
        } else {
            // Insertion only
            const preview = insertedText.length > 50 ? insertedText.substring(0, 50) + '...' : insertedText;
            return {
                type: 'insert',
                description: `Inserted "${preview}"`,
                added: insertedText.length,
                removed: 0,
                position: splice.pos
            };
        }
    },

    /**
     * Parse Redis protocol array response
     * @param {Uint8Array} data - Raw response data
     * @returns {Array<Uint8Array>} Array of change byte arrays
     */
    parseRedisArray(data) {
        const decoder = new TextDecoder('utf-8');
        const changes = [];
        let pos = 0;

        // Check if it's an array response: *N\r\n
        if (data[pos] !== 0x2a) { // '*'
            return changes;
        }

        // Find first \r\n to get array count
        let lineEnd = pos;
        while (lineEnd < data.length - 1) {
            if (data[lineEnd] === 0x0d && data[lineEnd + 1] === 0x0a) {
                break;
            }
            lineEnd++;
        }

        const countStr = decoder.decode(data.slice(pos + 1, lineEnd));
        const count = parseInt(countStr, 10);
        pos = lineEnd + 2; // Skip \r\n

        // Parse each bulk string: $len\r\n<data>\r\n
        for (let i = 0; i < count; i++) {
            if (data[pos] !== 0x24) { // '$'
                break;
            }

            // Find \r\n to get length
            lineEnd = pos;
            while (lineEnd < data.length - 1) {
                if (data[lineEnd] === 0x0d && data[lineEnd + 1] === 0x0a) {
                    break;
                }
                lineEnd++;
            }

            const lenStr = decoder.decode(data.slice(pos + 1, lineEnd));
            const len = parseInt(lenStr, 10);
            pos = lineEnd + 2; // Skip \r\n

            // Extract data
            const changeBytes = data.slice(pos, pos + len);
            changes.push(changeBytes);
            pos += len + 2; // Skip data and \r\n
        }

        return changes;
    },

    /**
     * Display history items in the sidebar
     * @param {Array} historyItems - Array of history item objects
     */
    displayHistory(historyItems) {
        const listDiv = document.getElementById('history-list');

        if (historyItems.length === 0) {
            listDiv.innerHTML = '<div style="padding: 20px; text-align: center; color: #999;">No history yet</div>';
            return;
        }

        listDiv.innerHTML = historyItems.map(item => {
            const time = new Date(item.timestamp).toLocaleTimeString();
            const actorShort = item.actor.toString().slice(0, 8);

            // Format change details
            let changeHtml = '';
            if (item.changeDetails) {
                const details = item.changeDetails;
                const typeIcon = {
                    'insert': '✚',
                    'delete': '✖',
                    'replace': '↻',
                    'none': '○'
                }[details.type] || '•';

                const typeColor = {
                    'insert': '#10b981',
                    'delete': '#ef4444',
                    'replace': '#f59e0b',
                    'none': '#9ca3af'
                }[details.type] || '#666';

                changeHtml = `
                    <div class="history-item-change" style="margin-top: 6px; padding: 6px; background: #f9fafb; border-radius: 4px; font-size: 11px;">
                        <div style="color: ${typeColor}; font-weight: bold; margin-bottom: 3px;">
                            ${typeIcon} ${details.type.toUpperCase()}
                        </div>
                        <div style="color: #666; word-break: break-word;">
                            ${details.description}
                        </div>
                        ${details.added > 0 || details.removed > 0 ? `
                            <div style="margin-top: 3px; font-size: 10px; color: #999;">
                                ${details.added > 0 ? `<span style="color: #10b981;">+${details.added}</span>` : ''}
                                ${details.removed > 0 ? `<span style="color: #ef4444;"> -${details.removed}</span>` : ''}
                            </div>
                        ` : ''}
                    </div>
                `;
            }

            return `
                <div class="history-item">
                    <div class="history-item-header">
                        <span class="history-item-index">#${item.index}</span>
                        <span class="history-item-time">${time}</span>
                    </div>
                    <div class="history-item-actor">Actor: ${actorShort}</div>
                    ${changeHtml}
                </div>
            `;
        }).join('');
    },

    /**
     * Prepend a new history item to the top of the list
     * @param {Object} item - History item object
     */
    prependHistoryItem(item) {
        const listDiv = document.getElementById('history-list');

        // Remove "no history" message if present
        if (listDiv.innerHTML.includes('No history yet')) {
            listDiv.innerHTML = '';
        }

        const time = new Date(item.timestamp).toLocaleTimeString();
        const actorShort = item.actor.toString().slice(0, 8);

        // Format change details
        let changeHtml = '';
        if (item.changeDetails) {
            const details = item.changeDetails;
            const typeIcon = {
                'insert': '✚',
                'delete': '✖',
                'replace': '↻',
                'none': '○'
            }[details.type] || '•';

            const typeColor = {
                'insert': '#10b981',
                'delete': '#ef4444',
                'replace': '#f59e0b',
                'none': '#9ca3af'
            }[details.type] || '#666';

            changeHtml = `
                <div class="history-item-change" style="margin-top: 6px; padding: 6px; background: #f9fafb; border-radius: 4px; font-size: 11px;">
                    <div style="color: ${typeColor}; font-weight: bold; margin-bottom: 3px;">
                        ${typeIcon} ${details.type.toUpperCase()}
                    </div>
                    <div style="color: #666; word-break: break-word;">
                        ${details.description}
                    </div>
                    ${details.added > 0 || details.removed > 0 ? `
                        <div style="margin-top: 3px; font-size: 10px; color: #999;">
                            ${details.added > 0 ? `<span style="color: #10b981;">+${details.added}</span>` : ''}
                            ${details.removed > 0 ? `<span style="color: #ef4444;"> -${details.removed}</span>` : ''}
                        </div>
                    ` : ''}
                </div>
            `;
        }

        const itemHtml = `
            <div class="history-item">
                <div class="history-item-header">
                    <span class="history-item-index">#${item.index}</span>
                    <span class="history-item-time">${time}</span>
                </div>
                <div class="history-item-actor">Actor: ${actorShort}</div>
                ${changeHtml}
            </div>
        `;

        listDiv.insertAdjacentHTML('afterbegin', itemHtml);
    },


    /**
     * Handle incoming change messages from Redis pub/sub
     * @param {string} messageData - Base64-encoded change bytes
     */
    handleIncomingMessage(messageData) {
        try {
            // Get old text before applying change
            const oldText = this.doc.text ? this.doc.text.toString() : '';

            const changeBytes = Uint8Array.from(atob(messageData), c => c.charCodeAt(0));
            const [newDoc] = Automerge.applyChanges(this.doc, [changeBytes]);

            // Get new text after applying change
            const newText = newDoc.text ? newDoc.text.toString() : '';

            this.doc = newDoc;

            this.updateEditor();
            this.updateDocInfo();

            // Only update history if the text actually changed
            // (Skip if this is our own change being echoed back)
            if (oldText !== newText) {
                // Calculate what changed
                const changeDetails = this.calculateChangeDetails(oldText, newText);

                // Update history sidebar with new change
                const history = Automerge.getHistory(this.doc);
                const latestChange = history[history.length - 1];

                this.prependHistoryItem({
                    index: history.length,
                    timestamp: latestChange.change.time || Date.now(),
                    actor: latestChange.change.actor || 'unknown',
                    message: latestChange.change.message || 'Document change',
                    changeDetails: changeDetails
                });
            }
        } catch (error) {
            console.error('[ShareableMode] Error applying change:', error);
            EditorCore.log(`Error handling message: ${error.message}`, 'error');
        }
    },

    /**
     * Handle keyspace notification events
     * Note: All changes are already handled via the changes: pub/sub channel
     * This is kept for potential future use (e.g., detecting external deletions)
     * @param {string} docKey - The document key
     * @param {string} command - The Redis command that triggered the notification
     */
    async handleKeyspaceNotification(docKey, command) {
        EditorCore.log(`Keyspace notification: ${command} on ${docKey}`, 'shareable');
        // No action needed - all changes come via pub/sub channel
    },

    /**
     * Handle local editor changes
     */
    async handleEdit() {
        if (!this.currentRoom) return;

        const docKey = `am:room:${this.currentRoom}`;
        const textarea = document.getElementById('editor-shareable');
        const newText = textarea.value;
        const oldText = this.prevText;

        // Calculate the minimal splice operation
        const splice = EditorCore.calculateSplice(oldText, newText);

        if (!splice) {
            return;
        }

        // Mark that we're actively editing
        this.isLocallyEditing = true;

        EditorCore.log(`Local edit: pos=${splice.pos}, del=${splice.del}, insert="${splice.text.substring(0, 20)}${splice.text.length > 20 ? '...' : ''}"`, 'shareable');

        // Apply to server - server will create Automerge change and broadcast it
        await EditorCore.applySpliceToServer(docKey, splice);

        // Only update prevText AFTER we've sent to server
        // This ensures we track what we actually sent
        this.prevText = newText;

        // Clear editing flag after a short delay (to handle echo-back)
        setTimeout(() => {
            this.isLocallyEditing = false;
        }, 100);
    },

    /**
     * Update the editor textarea
     */
    updateEditor() {
        const textarea = document.getElementById('editor-shareable');

        // Only skip if we're actively making local edits
        // Always apply changes from other editors
        if (this.isLocallyEditing && document.activeElement === textarea) {
            EditorCore.log(`Skipping update - locally editing`, 'shareable');
            return;
        }

        const currentText = textarea.value;
        // Convert Automerge Text object to plain string
        const newText = this.doc.text ? this.doc.text.toString() : '';
        const cursorPos = textarea.selectionStart;

        if (newText !== currentText) {
            const splice = EditorCore.calculateSplice(currentText, newText);
            textarea.value = newText;

            if (splice) {
                let newCursorPos = cursorPos;
                // Only adjust cursor if textarea is focused
                if (document.activeElement === textarea) {
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
            }

            this.prevText = newText;
            EditorCore.log(`Editor updated with remote changes`, 'shareable');
        }
    },

    /**
     * Update document info panel
     */
    updateDocInfo() {
        if (!this.doc) return;

        const history = Automerge.getHistory(this.doc);
        const textLength = this.doc.text ? this.doc.text.toString().length : 0;
        document.getElementById('doc-version-shareable').textContent = `v${history.length} (${textLength} chars)`;
    },

    /**
     * Disconnect from current room
     */
    async disconnect() {
        // Stop heartbeat
        if (this.heartbeatInterval) {
            clearInterval(this.heartbeatInterval);
            this.heartbeatInterval = null;
        }

        if (this.socket) {
            this.socket.close();
            this.socket = null;
        }

        this.doc = null;
        this.currentRoom = null;
        this.prevText = '';

        // Update UI
        document.getElementById('current-room-name').textContent = 'Not connected';
        document.getElementById('editor-shareable').value = '';
        document.getElementById('sync-status-shareable-editor').textContent = 'Not syncing';

        // Remove room from URL
        const url = new URL(window.location);
        url.searchParams.delete('room');
        window.history.pushState({}, '', url);

        EditorCore.log('Disconnected from room', 'shareable');

        // Trigger immediate room list refresh to update user count
        await this.triggerRoomListRefresh();
    },

    /**
     * Copy shareable link to clipboard
     */
    async copyShareableLink() {
        if (!this.currentRoom) return;

        const url = new URL(window.location);
        url.searchParams.set('room', this.currentRoom);
        const link = url.toString();

        // Find the button that was clicked
        const buttons = document.querySelectorAll('.toolbar-actions button');
        let copyButton = null;
        for (const btn of buttons) {
            if (btn.textContent.includes('Copy Link') || btn.textContent.includes('Link Copied')) {
                copyButton = btn;
                break;
            }
        }

        try {
            await navigator.clipboard.writeText(link);
            EditorCore.log('Shareable link copied to clipboard', 'shareable');

            // Provide visual feedback
            if (copyButton) {
                const originalText = copyButton.innerHTML;
                const originalBg = copyButton.style.background;

                copyButton.innerHTML = '✓ Link Copied';
                copyButton.classList.add('copy-success');

                // Revert after 2.5 seconds
                setTimeout(() => {
                    copyButton.innerHTML = originalText;
                    copyButton.classList.remove('copy-success');
                }, 2500);
            }
        } catch (error) {
            // Fallback for browsers that don't support clipboard API
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

            // Get active user counts and peer lists
            const roomsWithUsers = await Promise.all(
                rooms.map(async (roomName) => {
                    // Channel format: changes:am:room:{roomName}
                    const docKey = `am:room:${roomName}`;
                    const channel = `changes:${docKey}`;
                    const numsubResponse = await fetch(`${EditorCore.WEBDIS_URL}/PUBSUB/NUMSUB/${channel}`);
                    const numsubData = await numsubResponse.json();

                    let activeUsers = 0;
                    // PUBSUB NUMSUB returns array: [channel_name, subscriber_count, ...]
                    // Webdis returns it as 'PUBSUB' not 'PUBSUB NUMSUB'
                    const pubsubData = numsubData['PUBSUB NUMSUB'] || numsubData['PUBSUB'];
                    if (pubsubData && Array.isArray(pubsubData)) {
                        activeUsers = pubsubData[1] || 0;
                    }

                    // Get peer list for this room
                    const peers = await this.getPeersInRoom(roomName);

                    // Track peer changes to update activity timestamp
                    const previousPeers = this.roomPeers[roomName] || new Set();
                    const currentPeersSet = new Set(peers);

                    // Check if peers changed (joined or left)
                    const peersChanged =
                        previousPeers.size !== currentPeersSet.size ||
                        [...previousPeers].some(p => !currentPeersSet.has(p));

                    if (peersChanged) {
                        this.roomActivity[roomName] = Date.now();
                        this.roomPeers[roomName] = currentPeersSet;
                    }

                    // Ensure room has an activity timestamp (default to 0 for new rooms)
                    if (!this.roomActivity[roomName]) {
                        this.roomActivity[roomName] = 0;
                    }

                    return {
                        roomName,
                        activeUsers,
                        peers,
                        lastActivity: this.roomActivity[roomName]
                    };
                })
            );

            // Sort by most recent activity (newest first)
            this.roomList = roomsWithUsers.sort((a, b) => b.lastActivity - a.lastActivity);
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

        listDiv.innerHTML = this.roomList.map(({ roomName, activeUsers, peers }) => {
            const userClass = activeUsers > 0 ? 'active' : '';
            const userText = activeUsers === 1 ? '1 user' : `${activeUsers} users`;

            // Show peers if this is the current room
            const isCurrentRoom = this.currentRoom === roomName;
            const peerListHtml = isCurrentRoom && peers && peers.length > 0
                ? `<div class="peer-list">${peers.map(peerId =>
                    `<div class="peer-item">${peerId.slice(0, 8)}</div>`
                  ).join('')}</div>`
                : '';

            return `
                <div class="room-item-container ${isCurrentRoom ? 'current-room' : ''}">
                    <div class="room-item" onclick="ShareableMode.joinRoom('${roomName}')">
                        <span class="room-name">${roomName}</span>
                        <span class="room-users ${userClass}">${userText}</span>
                    </div>
                    ${peerListHtml}
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
    },

    /**
     * Start periodic room list refresh with smart debouncing
     */
    startRoomListRefresh() {
        let lastRefresh = Date.now();
        const minInterval = 2000; // Minimum 2 seconds between refreshes

        // Refresh every 5 seconds normally
        this.refreshInterval = setInterval(async () => {
            const now = Date.now();
            if (now - lastRefresh >= minInterval) {
                await this.refreshRoomList();
                lastRefresh = now;
            }
        }, 5000);
    },

    /**
     * Trigger an immediate room list refresh (debounced)
     */
    async triggerRoomListRefresh() {
        // Debounce rapid calls
        clearTimeout(this._refreshDebounce);
        this._refreshDebounce = setTimeout(async () => {
            await this.refreshRoomList();
        }, 500);
    },

    /**
     * Announce presence in a room
     */
    async announcePresence(roomName) {
        try {
            const presenceKey = `presence:${roomName}`;
            // Use Redis SET with expiration to track presence
            await fetch(`${EditorCore.WEBDIS_URL}/SETEX/${presenceKey}:${this.peerId}/10/${this.peerId}`);
            EditorCore.log(`Announced presence in ${roomName}`, 'shareable');
        } catch (error) {
            EditorCore.log(`Error announcing presence: ${error.message}`, 'error');
        }
    },

    /**
     * Start heartbeat to maintain presence
     */
    startHeartbeat(roomName) {
        // Announce presence every 5 seconds
        this.heartbeatInterval = setInterval(async () => {
            if (this.currentRoom === roomName) {
                await this.announcePresence(roomName);
            }
        }, 5000);
    },

    /**
     * Get peers in a room
     */
    async getPeersInRoom(roomName) {
        try {
            const pattern = `presence:${roomName}:*`;
            const response = await fetch(`${EditorCore.WEBDIS_URL}/KEYS/${pattern}`);
            const data = await response.json();

            const keys = data.KEYS || [];
            const peerIds = keys.map(key => key.split(':').pop());
            return peerIds;
        } catch (error) {
            EditorCore.log(`Error getting peers: ${error.message}`, 'error');
            return [];
        }
    }
};

// Make ShareableMode globally available
window.ShareableMode = ShareableMode;

// Use EditorCore constants
const WEBDIS_URL = EditorCore.WEBDIS_URL;
const WEBDIS_WS_URL = EditorCore.WEBDIS_WS_URL;
const SYNC_CHANNEL = 'automerge:sync';

// Editor state
let leftDoc = null;
let rightDoc = null;
let leftPeerId = EditorCore.generatePeerId();
let rightPeerId = EditorCore.generatePeerId();
let isConnected = false;
let isSyncing = false;

// WebSocket connections for pub/sub
let leftSocket = null;
let rightSocket = null;

// Track previous text state for calculating diffs
let leftPrevText = '';
let rightPrevText = '';

// Track if we're actively editing
let leftIsEditing = false;
let rightIsEditing = false;

/**
 * Generate a unique peer ID for this editor instance.
 * Used to identify which peer made changes and to filter out our own messages.
 * @returns {string} A unique peer ID string
 */
function generatePeerId() {
    return 'peer-' + Math.random().toString(36).substring(2, 15);
}

// Initialize
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

    // Set up editor event listeners
    const leftEditor = document.getElementById('editor-left');
    const rightEditor = document.getElementById('editor-right');

    leftEditor.addEventListener('input', EditorCore.debounce(() => handleEdit('left'), 300));
    rightEditor.addEventListener('input', EditorCore.debounce(() => handleEdit('right'), 300));

    // Update editors when they lose focus (to sync any pending changes)
    leftEditor.addEventListener('blur', () => {
        log('[left] Editor lost focus - syncing any pending changes', 'left');
        updateEditor('left');
    });
    rightEditor.addEventListener('blur', () => {
        log('[right] Editor lost focus - syncing any pending changes', 'right');
        updateEditor('right');
    });

    // Check connection periodically
    setInterval(checkConnection, 5000);
});

/**
 * Debounce function to limit how often a function can be called.
 * Used to delay processing of editor input events until typing pauses.
 * @param {Function} func - The function to debounce
 * @param {number} wait - Milliseconds to wait before calling func
 * @returns {Function} Debounced version of func
 */
function debounce(func, wait) {
    let timeout;
    return function executedFunction(...args) {
        const later = () => {
            clearTimeout(timeout);
            func(...args);
        };
        clearTimeout(timeout);
        timeout = setTimeout(later, wait);
    };
}

/**
 * Calculate the difference between two strings and return splice operation parameters.
 * Finds the common prefix and suffix to determine the minimal change.
 * @param {string} oldText - The previous text
 * @param {string} newText - The new text
 * @returns {Object|null} Object with {pos, del, text} or null if no change
 */
function calculateSplice(oldText, newText) {
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
}

/**
 * Append a log message to the sync log display.
 * Used throughout the app to show sync events, errors, and status changes.
 * @param {string} message - The message to log
 * @param {string} source - Source of the message (server, left, right, error)
 */
function log(message, source = 'server') {
    const logDiv = document.getElementById('sync-log');
    const timestamp = new Date().toLocaleTimeString();
    const className = `log-${source}`;
    logDiv.innerHTML += `<span class="${className}">[${timestamp}] [${source.toUpperCase()}] ${message}</span>\n`;
    logDiv.scrollTop = logDiv.scrollHeight;
}

/**
 * Clear all messages from the sync log display.
 * Called when user clicks the "Clear" button.
 */
function clearLog() {
    document.getElementById('sync-log').innerHTML = '';
}

/**
 * Select all text in the sync log display.
 * Called when user clicks the "Select All" button.
 */
function selectAllLog() {
    const logDiv = document.getElementById('sync-log');
    const range = document.createRange();
    range.selectNodeContents(logDiv);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
}

/**
 * Check if Webdis/Redis is reachable by sending a PING command.
 * Called on page load and periodically every 5 seconds.
 * Updates the connection status indicator.
 */
async function checkConnection() {
    try {
        const response = await fetch(`${WEBDIS_URL}/PING`);
        const data = await response.json();
        const connected = data.PING === 'PONG' || data.PING === true || (Array.isArray(data.PING) && data.PING[1] === 'PONG');
        updateConnectionStatus(connected);
    } catch (error) {
        updateConnectionStatus(false);
    }
}

/**
 * Update the UI connection status indicator.
 * Stops sync if connection is lost.
 * @param {boolean} connected - Whether Redis is connected
 */
function updateConnectionStatus(connected) {
    const status = document.getElementById('connection-status');
    const statusShareableEditor = document.getElementById('connection-status-shareable-editor');
    isConnected = connected;
    if (connected) {
        status.textContent = 'Connected';
        status.className = 'status-connected';
        if (statusShareableEditor) {
            statusShareableEditor.textContent = 'Connected';
            statusShareableEditor.className = 'status-connected';
        }
    } else {
        status.textContent = 'Disconnected';
        status.className = 'status-disconnected';
        if (statusShareableEditor) {
            statusShareableEditor.textContent = 'Disconnected';
            statusShareableEditor.className = 'status-disconnected';
        }
        if (isSyncing) {
            stopSync();
        }
    }
}

/**
 * Initialize a new Automerge document.
 * Creates empty documents for both editors and stores initial state in Redis.
 * Called when user clicks "Create New Document" button.
 */
async function initializeDocument() {
    const docKey = document.getElementById('doc-key').value;
    if (!docKey) {
        alert('Please enter a document key');
        return;
    }

    if (typeof Automerge === 'undefined') {
        alert('Automerge library is not loaded. Please refresh the page.');
        log('Automerge not loaded', 'error');
        return;
    }

    log(`Initializing document: ${docKey}`, 'server');

    // Create ONE Automerge document and save it to Redis
    // Editors will load from Redis when they connect (Approach B)
    const baseDoc = Automerge.init();
    const initialDoc = Automerge.change(baseDoc, doc => {
        doc.text = '';
    });

    // Create server document (single source of truth)
    try {
        // Create the document in Redis using AM.NEW (starts empty)
        const newResponse = await fetch(`${WEBDIS_URL}/AM.NEW/${docKey}`);
        const newData = await newResponse.json();
        log(`Document created in Redis: ${JSON.stringify(newData)}`, 'server');

        // Initialize the text field with empty string so AM.SPLICETEXT can work
        // Use GET with empty string parameter since Webdis PUT doesn't handle empty body well
        const initResponse = await fetch(`${WEBDIS_URL}/AM.PUTTEXT/${docKey}/text/`);
        const initData = await initResponse.json();
        log(`Text field initialized: ${JSON.stringify(initData)}`, 'server');

        log('Server document created successfully', 'server');
        log('Click "Connect Editors" to load and start syncing', 'server');
    } catch (error) {
        log(`Error initializing document: ${error.message}`, 'error');
    }
}

/**
 * Connect both editors to the shared document and start syncing.
 * Loads existing document from Redis (if available) and sets up WebSocket subscriptions.
 * Called when user clicks "Connect Editors" button.
 */
async function connectEditors() {
    const docKey = document.getElementById('doc-key').value;
    if (!docKey) {
        alert('Please enter a document key');
        return;
    }

    if (!isConnected) {
        alert('Not connected to Redis. Please check connection.');
        return;
    }

    log(`Connecting editors to document: ${docKey}`, 'server');

    // ALWAYS load initial state from Redis (Approach B - single source of truth)
    try {
        const loaded = await loadFromRedis(docKey);
        if (!loaded) {
            alert('No document found in Redis. Please create a new document first.');
            log('Connection failed: Document not found in Redis', 'error');
            return;
        }
    } catch (error) {
        alert(`Failed to load document: ${error.message}`);
        log(`Connection failed: ${error.message}`, 'error');
        return;
    }

    // Set up WebSocket-based subscriptions
    setupWebSocket('left', docKey);
    setupWebSocket('right', docKey);

    isSyncing = true;
    document.getElementById('sync-status').textContent = 'Syncing ✓';
    log('Editors connected with WebSocket-based sync', 'server');
}

/**
 * Set up WebSocket connection for real-time pub/sub synchronization.
 * Subscribes to the document's sync channel and handles incoming change messages.
 * Automatically reconnects if connection is lost.
 * @param {string} editor - Which editor ('left' or 'right')
 * @param {string} docKey - The document key to subscribe to
 */
function setupWebSocket(editor, docKey) {
    const channel = `changes:${docKey}`;
    const keyspaceChannel = `__keyspace@0__:${docKey}`;
    const peerId = editor === 'left' ? leftPeerId : rightPeerId;

    // Create WebSocket connection - Webdis uses /.json endpoint for WebSocket
    const ws = new WebSocket(`${WEBDIS_WS_URL}/.json`);

    ws.onopen = () => {
        log(`[${editor}] WebSocket connected`, editor);

        // Subscribe to the changes channel (for server-published changes)
        const subscribeCmd = JSON.stringify(['SUBSCRIBE', channel]);
        ws.send(subscribeCmd);
        log(`[${editor}] Subscribed to ${channel}`, editor);

        // Subscribe to keyspace notifications (for external changes to the Redis key)
        const keyspaceCmd = JSON.stringify(['SUBSCRIBE', keyspaceChannel]);
        ws.send(keyspaceCmd);
        log(`[${editor}] Subscribed to ${keyspaceChannel}`, editor);
    };

    ws.onmessage = (event) => {
        try {
            const response = JSON.parse(event.data);

            if (response.SUBSCRIBE) {
                const data = response.SUBSCRIBE;
                if (data[0] === 'subscribe') {
                    log(`[${editor}] Subscription confirmed for ${data[1]}`, editor);
                } else if (data[0] === 'message') {
                    const channelName = data[1];
                    const messageData = data[2];

                    // Check if this is a keyspace notification or server-published change
                    if (channelName.startsWith('__keyspace@')) {
                        // Keyspace notification - external change to the Redis key
                        log(`[${editor}] Keyspace event: ${messageData}`, editor);
                        handleKeyspaceNotification(editor, docKey, messageData);
                    } else {
                        // Server-published change message
                        log(`[${editor}] Received pub/sub message`, editor);
                        handleIncomingMessage(editor, messageData, peerId);
                    }
                }
            } else if (response.MESSAGE) {
                const data = response.MESSAGE;
                if (data[0] === 'message') {
                    const channelName = data[1];
                    const messageData = data[2];

                    // Check if this is a keyspace notification or regular pub/sub message
                    if (channelName.startsWith('__keyspace@')) {
                        // Keyspace notification - external change to the Redis key
                        log(`[${editor}] Keyspace event: ${messageData}`, editor);
                        handleKeyspaceNotification(editor, docKey, messageData);
                    } else {
                        // Server-published change message
                        log(`[${editor}] Received pub/sub message`, editor);
                        handleIncomingMessage(editor, messageData, peerId);
                    }
                }
            } else {
                log(`[${editor}] Unknown message type: ${JSON.stringify(response)}`, editor);
            }
        } catch (error) {
            log(`[${editor}] WebSocket message error: ${error.message}`, 'error');
            console.error('WebSocket message error:', error, event.data);
        }
    };

    ws.onerror = (error) => {
        log(`[${editor}] WebSocket error`, 'error');
        console.error(`[${editor}] WebSocket error:`, error);
    };

    ws.onclose = (event) => {
        log(`[${editor}] WebSocket disconnected (code: ${event.code}, reason: ${event.reason})`, editor);
        console.log(`[${editor}] WebSocket close event:`, event);

        // Attempt to reconnect after 2 seconds
        if (isSyncing) {
            setTimeout(() => {
                log(`[${editor}] Attempting to reconnect...`, editor);
                setupWebSocket(editor, docKey);
            }, 2000);
        }
    };

    // Store WebSocket reference
    if (editor === 'left') {
        leftSocket = ws;
    } else {
        rightSocket = ws;
    }
}

/**
 * Handle keyspace notification events from Redis.
 * Note: All changes are already handled via the changes: pub/sub channel
 * This is kept for potential future use (e.g., detecting external deletions)
 * @param {string} editor - Which editor ('left' or 'right')
 * @param {string} docKey - The document key
 * @param {string} command - The Redis command that triggered the notification (e.g., "am.splicetext")
 */
async function handleKeyspaceNotification(editor, docKey, command) {
    log(`[${editor}] Keyspace notification: ${command} on ${docKey}`, editor);
    // No action needed - all changes come via pub/sub channel
}

/**
 * Handle incoming change messages from Redis pub/sub via WebSocket.
 * Decodes the base64-encoded change bytes from the server and applies them to the local document.
 * @param {string} editor - Which editor ('left' or 'right')
 * @param {string} messageData - Base64-encoded change bytes from server
 * @param {string} myPeerId - This peer's ID (not used anymore, kept for compatibility)
 */
function handleIncomingMessage(editor, messageData, myPeerId) {
    try {
        // Decode the change bytes from base64 (server publishes raw change bytes)
        const changeBytes = Uint8Array.from(atob(messageData), c => c.charCodeAt(0));

        // Get current document
        const currentDoc = editor === 'left' ? leftDoc : rightDoc;

        // Apply the change to the current document
        const [newDoc] = Automerge.applyChanges(currentDoc, [changeBytes]);

        // Update the document reference
        if (editor === 'left') {
            leftDoc = newDoc;
        } else {
            rightDoc = newDoc;
        }

        // Update the editor UI
        updateEditor(editor);
        updateDocInfo(editor);

    } catch (error) {
        log(`[${editor}] Error applying change: ${error.message}`, 'error');
        console.error('Error handling message:', error);
    }
}

/**
 * Stop synchronization and close all WebSocket connections.
 * Called when connection is lost or user manually stops sync.
 */
function stopSync() {
    if (leftSocket) {
        leftSocket.close();
        leftSocket = null;
    }
    if (rightSocket) {
        rightSocket.close();
        rightSocket = null;
    }
    isSyncing = false;
    document.getElementById('sync-status').textContent = 'Not syncing';
    log('Sync stopped', 'server');
}

/**
 * Handle local editor changes and sync to server.
 * Calculates the minimal diff and applies to server via AM.SPLICETEXT.
 * Server automatically publishes changes to the changes:{key} channel.
 * Debounced to avoid excessive updates during rapid typing.
 * Preserves cursor position by tracking change location.
 * @param {string} editor - Which editor ('left' or 'right')
 */
async function handleEdit(editor) {
    if (!isSyncing) return;

    const docKey = document.getElementById('doc-key').value;
    const textarea = document.getElementById(`editor-${editor}`);
    const newText = textarea.value;

    // Get previous text for this editor
    const oldText = editor === 'left' ? leftPrevText : rightPrevText;

    // Calculate the minimal splice operation
    const splice = EditorCore.calculateSplice(oldText, newText);

    if (!splice) {
        // No change detected
        return;
    }

    // Mark that we're actively editing
    if (editor === 'left') {
        leftIsEditing = true;
    } else {
        rightIsEditing = true;
    }

    log(`[${editor}] Local edit: pos=${splice.pos}, del=${splice.del}, insert="${splice.text.substring(0, 20)}${splice.text.length > 20 ? '...' : ''}"`, editor);

    // Apply changes to server document via AM.SPLICETEXT
    // Server will automatically publish changes to the changes:{key} channel
    await applySpliceToServer(docKey, splice);

    // Only update prevText AFTER we've sent to server
    // This ensures we track what we actually sent
    if (editor === 'left') {
        leftPrevText = newText;
    } else {
        rightPrevText = newText;
    }

    // Clear editing flag after a short delay (to handle echo-back)
    setTimeout(() => {
        if (editor === 'left') {
            leftIsEditing = false;
        } else {
            rightIsEditing = false;
        }
    }, 100);
}

/**
 * Apply a splice operation to the server document.
 * Uses AM.SPLICETEXT to apply incremental changes efficiently.
 * Server automatically publishes changes to the changes:{key} channel.
 * @param {string} docKey - The document key
 * @param {Object} splice - The splice operation {pos, del, text}
 */
async function applySpliceToServer(docKey, splice) {
    try {
        let response;
        if (splice.text === '') {
            // Webdis doesn't send empty PUT body as an argument to Redis
            // Use GET with trailing slash for empty string (deletions)
            const url = `${WEBDIS_URL}/AM.SPLICETEXT/${docKey}/text/${splice.pos}/${splice.del}/`;
            response = await fetch(url);
        } else {
            // For insertions/replacements, use PUT with text in body
            const url = `${WEBDIS_URL}/AM.SPLICETEXT/${docKey}/text/${splice.pos}/${splice.del}`;
            response = await fetch(url, {
                method: 'PUT',
                headers: {
                    'Content-Type': 'text/plain'
                },
                body: splice.text
            });
        }

        const data = await response.json();

        if (data['AM.SPLICETEXT'] && (data['AM.SPLICETEXT'] === 'OK' || data['AM.SPLICETEXT'][0] === true)) {
            // Success
        } else {
            log(`AM.SPLICETEXT error: ${JSON.stringify(data)}`, 'error');
        }
    } catch (error) {
        log(`Error applying splice to server: ${error.message}`, 'error');
        console.error('Server splice error:', error);
    }
}

/**
 * Initialize client documents from server.
 * Loads the authoritative document from Redis using AM.SAVE so all clients share the same document history.
 * Use .raw endpoint because Webdis .json endpoint can't handle binary data.
 * @param {string} docKey - The document key to load
 * @returns {boolean} True if initialization successful
 */
async function loadFromRedis(docKey) {
    try {
        // Load the document from Redis to ensure all clients share the same document history
        // Use .raw endpoint because Webdis .json endpoint can't handle binary data
        const response = await fetch(`${WEBDIS_URL}/AM.SAVE/${docKey}.raw`);

        if (response.ok) {
            // Get raw binary data as ArrayBuffer
            const arrayBuffer = await response.arrayBuffer();
            const docBytes = new Uint8Array(arrayBuffer);

            // Webdis .raw returns Redis protocol format: $NNN\r\n<data>\r\n
            // Parse the bulk string header to get exact byte count
            const decoder = new TextDecoder('utf-8');
            let headerEnd = 0;
            for (let i = 0; i < docBytes.length - 1; i++) {
                if (docBytes[i] === 0x0d && docBytes[i + 1] === 0x0a) {
                    headerEnd = i;
                    break;
                }
            }

            // Extract byte count from header: $518\r\n -> "518"
            const headerText = decoder.decode(docBytes.slice(1, headerEnd));
            const byteCount = parseInt(headerText, 10);
            const dataStart = headerEnd + 2; // Skip past \r\n

            // Extract exactly byteCount bytes (ignore trailing \r\n)
            const actualDocBytes = docBytes.slice(dataStart, dataStart + byteCount);

            // Both editors load the same document from server
            leftDoc = Automerge.load(actualDocBytes);
            rightDoc = Automerge.load(actualDocBytes);

            log(`Loaded document from server (${actualDocBytes.length} bytes, ${docBytes.length - actualDocBytes.length} header bytes)`, 'server');
        } else {
            log(`Failed to load document from server: ${response.status}`, 'error');
            return false;
        }

        // Initialize previous text state for diff tracking
        // Convert Automerge Text objects to plain strings
        leftPrevText = leftDoc.text ? leftDoc.text.toString() : '';
        rightPrevText = rightDoc.text ? rightDoc.text.toString() : '';

        updateEditor('left');
        updateEditor('right');
        updateDocInfo('left');
        updateDocInfo('right');

        log('Editors initialized from server document', 'server');
        return true;
    } catch (error) {
        log(`Error initializing editors: ${error.message}`, 'error');
        console.error('Load error:', error);
        throw error;
    }
}

/**
 * Update the editor textarea to reflect the current document state.
 * Intelligently preserves cursor position by calculating the splice that was applied.
 * Called after applying remote changes or loading document.
 * @param {string} editor - Which editor ('left' or 'right')
 */
function updateEditor(editor) {
    const doc = editor === 'left' ? leftDoc : rightDoc;
    if (!doc) return;

    const textarea = document.getElementById(`editor-${editor}`);
    const isEditing = editor === 'left' ? leftIsEditing : rightIsEditing;

    // Only skip if we're actively making local edits
    // Always apply changes from other editors
    if (isEditing && document.activeElement === textarea) {
        log(`[${editor}] Skipping update - locally editing`, editor);
        return;
    }

    const currentText = textarea.value;
    // Convert Automerge Text object to plain string
    const newText = doc.text ? doc.text.toString() : '';
    const cursorPos = textarea.selectionStart;
    const cursorEnd = textarea.selectionEnd;

    // Only update if text differs
    if (newText !== currentText) {
        // Calculate the splice to determine cursor adjustment
        const splice = EditorCore.calculateSplice(currentText, newText);

        textarea.value = newText;

        if (splice) {
            // Adjust cursor position based on the splice operation
            let newCursorPos = cursorPos;
            let newCursorEnd = cursorEnd;

            // Only adjust cursor if textarea is focused
            if (document.activeElement === textarea && cursorPos >= splice.pos) {
                // Calculate net change in text length
                const netChange = splice.text.length - splice.del;

                if (cursorPos <= splice.pos + splice.del) {
                    // Cursor was inside the deleted range, move it to end of insertion
                    newCursorPos = splice.pos + splice.text.length;
                } else {
                    // Cursor was after the deleted range, shift by net change
                    newCursorPos = cursorPos + netChange;
                }

                // Adjust selection end similarly
                if (cursorEnd <= splice.pos + splice.del) {
                    newCursorEnd = splice.pos + splice.text.length;
                } else {
                    newCursorEnd = cursorEnd + netChange;
                }
            }

            // Ensure cursor positions are within valid range
            newCursorPos = Math.max(0, Math.min(newCursorPos, newText.length));
            newCursorEnd = Math.max(0, Math.min(newCursorEnd, newText.length));

            // Only set selection if textarea is focused
            if (document.activeElement === textarea) {
                textarea.setSelectionRange(newCursorPos, newCursorEnd);
            }
        } else {
            // No splice calculated, try to preserve original position
            const safePos = Math.min(cursorPos, newText.length);
            textarea.setSelectionRange(safePos, safePos);
        }

        // Update previous text state for this editor
        if (editor === 'left') {
            leftPrevText = newText;
        } else {
            rightPrevText = newText;
        }

        log(`[${editor}] Editor updated from document`, editor);
    }
}

/**
 * Update the document info panel showing stats.
 * Displays character count, number of changes, and peer ID.
 * Called after any document modification.
 * @param {string} editor - Which editor ('left' or 'right')
 */
function updateDocInfo(editor) {
    const doc = editor === 'left' ? leftDoc : rightDoc;
    if (!doc) return;

    const infoDiv = document.getElementById(`info-${editor}`);
    const history = Automerge.getHistory(doc);
    // Convert Automerge Text object to plain string for length calculation
    const textLength = doc.text ? doc.text.toString().length : 0;

    infoDiv.innerHTML = `
        <div>Characters: ${textLength}</div>
        <div>Changes: ${history.length}</div>
        <div>Peer: ${editor === 'left' ? leftPeerId.slice(0, 8) : rightPeerId.slice(0, 8)}</div>
    `;

    // Update version display
    document.getElementById(`doc-version-${editor}`).textContent =
        `${editor === 'left' ? 'Left' : 'Right'}: v${history.length}`;
}

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
