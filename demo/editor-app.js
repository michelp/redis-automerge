// Webdis configuration
const WEBDIS_URL = 'http://localhost:7379';
const WEBDIS_WS_URL = 'ws://localhost:7379';
const SYNC_CHANNEL = 'automerge:sync';

// Editor state
let leftDoc = null;
let rightDoc = null;
let leftPeerId = generatePeerId();
let rightPeerId = generatePeerId();
let isConnected = false;
let isSyncing = false;

// WebSocket connections for pub/sub
let leftSocket = null;
let rightSocket = null;

// Track previous text state for calculating diffs
let leftPrevText = '';
let rightPrevText = '';

/**
 * Generate a unique peer ID for this editor instance.
 * Used to identify which peer made changes and to filter out our own messages.
 * @returns {string} A unique peer ID string
 */
function generatePeerId() {
    return 'peer-' + Math.random().toString(36).substring(2, 15);
}

// Initialize
window.addEventListener('load', () => {
    log('Collaborative editor loaded', 'server');

    // Check if Automerge loaded
    if (typeof Automerge === 'undefined') {
        log('ERROR: Automerge library failed to load!', 'error');
    } else {
        log('Automerge library ready', 'server');
    }

    checkConnection();

    // Set peer IDs
    document.getElementById('peer-id-left').textContent = `Peer ID: ${leftPeerId.slice(0, 8)}`;
    document.getElementById('peer-id-right').textContent = `Peer ID: ${rightPeerId.slice(0, 8)}`;

    // Set up editor event listeners
    const leftEditor = document.getElementById('editor-left');
    const rightEditor = document.getElementById('editor-right');

    leftEditor.addEventListener('input', debounce(() => handleEdit('left'), 300));
    rightEditor.addEventListener('input', debounce(() => handleEdit('right'), 300));

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
    isConnected = connected;
    if (connected) {
        status.textContent = 'Connected';
        status.className = 'status-connected';
    } else {
        status.textContent = 'Disconnected';
        status.className = 'status-disconnected';
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
            console.log(`[${editor}] WebSocket raw message:`, event.data);
            const response = JSON.parse(event.data);
            console.log(`[${editor}] WebSocket parsed data:`, response);

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
 * Called when an external client modifies the document.
 * @param {string} editor - Which editor ('left' or 'right')
 * @param {string} docKey - The document key
 * @param {string} command - The Redis command that triggered the notification (e.g., "am.splicetext")
 */
async function handleKeyspaceNotification(editor, docKey, command) {
    try {
        console.log(`[${editor}] Keyspace notification: ${command} on ${docKey}`);

        // Fetch the current text from the server
        const response = await fetch(`${WEBDIS_URL}/AM.GETTEXT/${docKey}/text`);
        const data = await response.json();

        if (data && data['AM.GETTEXT']) {
            const serverText = data['AM.GETTEXT'];
            console.log(`[${editor}] Fetched server text:`, serverText);

            // Get current document
            const currentDoc = editor === 'left' ? leftDoc : rightDoc;

            // Only update if the server text is different from local
            if (currentDoc && currentDoc.text !== serverText) {
                log(`[${editor}] External change detected, updating from server`, editor);

                // Update the local document
                const newDoc = Automerge.change(currentDoc, doc => {
                    doc.text = serverText;
                });

                // Update the document reference
                if (editor === 'left') {
                    leftDoc = newDoc;
                } else {
                    rightDoc = newDoc;
                }

                // Update the editor UI (will also update prevText)
                updateEditor(editor);
                updateDocInfo(editor);
            }
        }
    } catch (error) {
        log(`[${editor}] Error handling keyspace notification: ${error.message}`, 'error');
        console.error('Keyspace notification error:', error);
    }
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
        console.log(`[${editor}] handleIncomingMessage called with:`, messageData);

        // Decode the change bytes from base64 (server publishes raw change bytes)
        const changeBytes = Uint8Array.from(atob(messageData), c => c.charCodeAt(0));
        console.log(`[${editor}] Decoded change bytes, length:`, changeBytes.length);

        log(`[${editor}] Received change from server`, editor);

        // Get current document
        const currentDoc = editor === 'left' ? leftDoc : rightDoc;
        console.log(`[${editor}] Current doc:`, currentDoc);

        // Apply the change to the current document
        console.log(`[${editor}] About to apply change. CurrentDoc actor:`, Automerge.getActorId(currentDoc));
        const [newDoc] = Automerge.applyChanges(currentDoc, [changeBytes]);
        console.log(`[${editor}] ApplyChanges returned successfully`);
        console.log(`[${editor}] New doc after applying change:`, newDoc);

        // Update the document reference
        if (editor === 'left') {
            leftDoc = newDoc;
        } else {
            rightDoc = newDoc;
        }

        // Update the editor UI
        updateEditor(editor);
        updateDocInfo(editor);

        log(`[${editor}] Applied change from server`, editor);

    } catch (error) {
        log(`[${editor}] Error handling message: ${error.message}`, 'error');
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
    const splice = calculateSplice(oldText, newText);

    if (!splice) {
        // No change detected
        return;
    }

    console.log(`[${editor}] Splice calculated:`, splice);

    // Get current document
    const oldDoc = editor === 'left' ? leftDoc : rightDoc;

    // Create new document with changes
    const newDoc = Automerge.change(oldDoc, d => {
        d.text = newText;
    });

    // Update the document reference
    if (editor === 'left') {
        leftDoc = newDoc;
        leftPrevText = newText;
    } else {
        rightDoc = newDoc;
        rightPrevText = newText;
    }

    log(`[${editor}] Local edit: pos=${splice.pos}, del=${splice.del}, insert="${splice.text.substring(0, 20)}${splice.text.length > 20 ? '...' : ''}"`, editor);

    // Apply changes to server document via AM.SPLICETEXT
    // Server will automatically publish changes to the changes:{key} channel
    await applySpliceToServer(docKey, splice);

    updateDocInfo(editor);
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
        // Use AM.SPLICETEXT via PUT to apply the splice operation
        // Webdis PUT format: PUT /COMMAND/arg1/arg2/... with body as last argument
        const response = await fetch(`${WEBDIS_URL}/AM.SPLICETEXT/${docKey}/text/${splice.pos}/${splice.del}`, {
            method: 'PUT',
            headers: {
                'Content-Type': 'text/plain'
            },
            body: splice.text
        });
        const data = await response.json();

        console.log('AM.SPLICETEXT response:', data);

        if (data['AM.SPLICETEXT'] && (data['AM.SPLICETEXT'] === 'OK' || data['AM.SPLICETEXT'][0] === true)) {
            // log('Server splice applied successfully', 'server');
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
 * For now, starts with empty documents - server sync happens via pub/sub.
 * Future: Use AM.SAVE when Webdis supports binary responses properly.
 * @param {string} docKey - The document key to load
 * @returns {boolean} True if initialization successful
 */
async function loadFromRedis(docKey) {
    try {
        // Initialize both editors with empty Automerge documents
        // They will sync via pub/sub as changes are applied
        const baseDoc = Automerge.init();
        const initialDoc = Automerge.change(baseDoc, doc => {
            doc.text = '';
        });

        leftDoc = Automerge.clone(initialDoc);
        rightDoc = Automerge.clone(initialDoc);

        // Initialize previous text state for diff tracking
        leftPrevText = '';
        rightPrevText = '';

        updateEditor('left');
        updateEditor('right');
        updateDocInfo('left');
        updateDocInfo('right');

        log('Editors initialized (will sync via pub/sub)', 'server');
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
    const oldText = textarea.value;
    const newText = doc.text || '';
    const cursorPos = textarea.selectionStart;
    const cursorEnd = textarea.selectionEnd;

    // Only update if text differs
    if (newText !== oldText) {
        // Calculate the splice to determine cursor adjustment
        const splice = calculateSplice(oldText, newText);

        textarea.value = newText;

        if (splice) {
            // Adjust cursor position based on the splice operation
            let newCursorPos = cursorPos;
            let newCursorEnd = cursorEnd;

            // If cursor is after the change position, adjust it
            if (cursorPos >= splice.pos) {
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

            textarea.setSelectionRange(newCursorPos, newCursorEnd);
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

    infoDiv.innerHTML = `
        <div>Characters: ${(doc.text || '').length}</div>
        <div>Changes: ${history.length}</div>
        <div>Peer: ${editor === 'left' ? leftPeerId.slice(0, 8) : rightPeerId.slice(0, 8)}</div>
    `;

    // Update version display
    document.getElementById(`doc-version-${editor}`).textContent =
        `${editor === 'left' ? 'Left' : 'Right'}: v${history.length}`;
}
