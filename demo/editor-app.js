// ============================================================================
// EditorCore - Shared primitives
// ============================================================================
const EditorCore = {
    WEBDIS_URL: `${window.location.protocol}//${window.location.host}/api`,
    WEBDIS_WS_URL: `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}/api`,


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
     * @param {string} title - The document title
     * @returns {Promise<boolean>} True if successful
     */
    async initializeDocument(docKey, title = 'Untitled Document') {
        try {
            this.log(`Initializing document: ${docKey}`, 'server');

            // Create the document in Redis using AM.NEW
            const newResponse = await fetch(`${this.WEBDIS_URL}/AM.NEW/${docKey}`);
            const newData = await newResponse.json();
            this.log(`Document created in Redis: ${JSON.stringify(newData)}`, 'server');

            // Initialize the title field
            const titleResponse = await fetch(`${this.WEBDIS_URL}/AM.PUTTEXT/${docKey}/title/${encodeURIComponent(title)}`);
            const titleData = await titleResponse.json();
            this.log(`Title field initialized: ${JSON.stringify(titleData)}`, 'server');

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
// ShareableMode - editor with document management
// ============================================================================
const ShareableMode = {
    doc: null,
    socket: null,
    documentListSocket: null,
    prevText: '',
    currentDocument: null, // Current document UUID
    currentDocumentTitle: null, // Current document title
    documentList: [],
    documentPeers: {}, // Track peers by document: { documentId: Set of actorIds }
    documentActivity: {}, // Track last activity timestamp by document: { documentId: timestamp }
    _currentHandler: null,
    _blurHandler: null,
    _titleHandler: null,
    _refreshDebounce: null,
    refreshInterval: null,
    heartbeatInterval: null,
    isLocallyEditing: false, // Track if we're actively making changes
    _displayedChatCount: 0, // Track how many chat messages have been displayed
    screenName: null, // User's screen name
    actorToScreenName: {}, // Map actor IDs to screen names

    /**
     * Get the actor ID from the current document
     * @returns {string|null} Actor ID or null if no document
     */
    getActorId() {
        if (!this.doc) return null;
        return Automerge.getActorId(this.doc);
    },

    /**
     * Get screen name for an actor ID
     * @param {string} actorId - The actor ID
     * @returns {string} Screen name or shortened actor ID if not found
     */
    getScreenName(actorId) {
        if (!actorId) return 'unknown';

        // Check if we have a screen name mapping
        if (this.actorToScreenName[actorId]) {
            return this.actorToScreenName[actorId];
        }

        // Check if it's in the document metadata
        if (this.doc && this.doc.user_metadata && this.doc.user_metadata[actorId]) {
            return this.doc.user_metadata[actorId].screenName || actorId.slice(0, 8);
        }

        // Fall back to shortened actor ID
        return actorId.slice(0, 8);
    },

    /**
     * Display user info with OAuth provider information
     */
    displayUserInfo() {
        // Use instance variables (no localStorage - security: prevent XSS token theft)
        const provider = this.provider || 'oauth';
        const avatarUrl = this.avatarUrl;

        // Create user info HTML with avatar if available
        let userInfoHtml = `<span style="font-weight: 600;">${this.screenName}</span>`;

        if (avatarUrl) {
            userInfoHtml = `
                <div style="display: inline-flex; align-items: center; gap: 8px;">
                    <img src="${avatarUrl}" alt="${this.screenName}" style="width: 24px; height: 24px; border-radius: 50%; border: 2px solid #667eea;" />
                    <span style="font-weight: 600;">${this.screenName}</span>
                    <span style="font-size: 11px; color: #999;">(${provider})</span>
                </div>
            `;
        } else {
            userInfoHtml += ` <span style="font-size: 11px; color: #999;">(${provider})</span>`;
        }

        // Update any user info displays in the UI
        const userInfoElements = document.querySelectorAll('.user-info-display');
        userInfoElements.forEach(el => {
            el.innerHTML = userInfoHtml;
        });

        EditorCore.log(`Authenticated via ${provider}`, 'shareable');
    },

    /**
     * Initialize shareable mode
     */
    async initialize() {
        // Check authentication using new OAuth-aware Auth module
        const auth = await Auth.requireAuth();
        if (!auth) {
            // requireAuth() will redirect to login if not authenticated
            return;
        }

        // Store user credentials (no localStorage - security: prevent XSS token theft)
        this.screenName = auth.screenName;
        this.stableActorId = auth.actorId;
        this.authToken = auth.token;
        this.provider = auth.provider;
        this.avatarUrl = auth.avatarUrl || '';

        EditorCore.log(`Logged in as: ${this.screenName} (actor: ${this.stableActorId.slice(0, 8)}...)`, 'shareable');

        // Display user info with OAuth provider
        this.displayUserInfo();

        // Check for document in URL
        const urlDocument = this.parseUrlDocument();
        if (urlDocument) {
            EditorCore.log(`Document from URL: ${urlDocument}`, 'shareable');
            // Auto-select shareable tab
            if (typeof switchMode !== 'undefined') {
                switchMode('shareable');
            }
            // Auto-join the document from URL
            await this.refreshDocumentList();
            await this.joinDocument(urlDocument);
        } else {
            await this.refreshDocumentList();
        }

        // Subscribe to document list updates
        this.subscribeToDocumentEvents();

        // Periodically refresh document list to update user counts (lightweight operation)
        this.startDocumentListRefresh();
    },

    /**
     * Parse document name from URL parameters
     * @returns {string|null} Document name or null
     */
    parseUrlDocument() {
        const params = new URLSearchParams(window.location.search);
        return params.get('document');
    },

    /**
     * Generate a UUID v4
     * @returns {string} UUID
     */
    generateUUID() {
        // Use crypto.randomUUID() if available (modern browsers)
        if (typeof crypto !== 'undefined' && crypto.randomUUID) {
            return crypto.randomUUID();
        }
        // Fallback for older browsers
        return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
            const r = Math.random() * 16 | 0;
            const v = c === 'x' ? r : (r & 0x3 | 0x8);
            return v.toString(16);
        });
    },

    /**
     * Create a new document with custom title
     */
    async createCustomDocument() {
        const input = document.getElementById('custom-document-name');
        const title = input.value.trim();

        if (!title) {
            alert('Please enter a document title');
            return;
        }

        if (title.length > 100) {
            alert('Document title must be 100 characters or less');
            return;
        }

        input.value = '';

        // Generate UUID for document ID
        const documentId = this.generateUUID();

        await this.createDocument(documentId, title);
    },

    /**
     * Create a new document
     * @param {string} documentId - The document UUID
     * @param {string} title - The document title
     */
    async createDocument(documentId, title = 'Untitled Document') {
        const docKey = `am:document:${documentId}`;

        EditorCore.log(`Creating document: ${title} (${documentId})`, 'shareable');

        // Check if document already exists (shouldn't happen with UUIDs, but check anyway)
        const exists = await this.checkDocumentExists(docKey);
        if (exists) {
            EditorCore.log(`Document ${documentId} already exists, joining instead`, 'shareable');
            await this.joinDocument(documentId);
            return;
        }

        // Create the document
        const success = await EditorCore.initializeDocument(docKey, title);
        if (success) {
            EditorCore.log(`Document created: ${title}`, 'shareable');
            await this.joinDocument(documentId);
        } else {
            alert('Failed to create document. Please try again.');
        }
    },

    /**
     * Check if a document exists in Redis
     * @param {string} docKey - The document key
     * @returns {Promise<boolean>} True if exists
     */
    async checkDocumentExists(docKey) {
        try {
            const response = await fetch(`${EditorCore.WEBDIS_URL}/EXISTS/${docKey}`);
            const data = await response.json();
            return data.EXISTS === 1 || data.EXISTS === true;
        } catch (error) {
            EditorCore.log(`Error checking document existence: ${error.message}`, 'error');
            return false;
        }
    },

    /**
     * Join an existing document
     * @param {string} documentId - The document UUID
     */
    async joinDocument(documentId) {
        // Check if already in this document
        if (this.currentDocument === documentId) {
            EditorCore.log(`Already connected to document: ${documentId}`, 'shareable');
            return;
        }

        // If already in another document, disconnect first
        if (this.currentDocument) {
            EditorCore.log(`Leaving document: ${this.currentDocument}`, 'shareable');

            // Reset editing flag
            this.isLocallyEditing = false;

            // Clear the heartbeat interval
            if (this.heartbeatInterval) {
                clearInterval(this.heartbeatInterval);
                this.heartbeatInterval = null;
            }

            // Close WebSocket
            if (this.socket) {
                this.socket.close();
                this.socket = null;
            }

            // Clear the editor textarea
            const editor = document.getElementById('editor-shareable');
            editor.value = '';

            // Clear history display
            document.getElementById('history-list').innerHTML =
                '<div style="padding: 20px; text-align: center; color: #999;">Loading history...</div>';

            // Reset chat count
            this._displayedChatCount = 0;
        }

        const docKey = `am:document:${documentId}`;

        EditorCore.log(`Joining document: ${documentId}`, 'shareable');

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

                // Load document with our stable actor ID
                this.doc = Automerge.load(actualDocBytes, { actorId: this.stableActorId });

                // Convert Automerge Text object to plain string
                const textValue = this.doc.text ? this.doc.text.toString() : '';
                this.prevText = textValue;

                // Load title from document
                this.currentDocumentTitle = this.doc.title ? this.doc.title.toString() : 'Untitled Document';

                EditorCore.log(`Loaded document from server (${actualDocBytes.length} bytes)`, 'shareable');
            } else {
                console.error('[ShareableMode] Failed to load document, status:', response.status);
                EditorCore.log('Failed to load document from server', 'error');
                // Initialize document with our stable actor ID
                const baseDoc = Automerge.init({ actorId: this.stableActorId });
                this.doc = Automerge.change(baseDoc, doc => {
                    doc.title = "Untitled Document";
                    doc.text = "";
                    doc.chat_history = [];
                });
                this.currentDocumentTitle = 'Untitled Document';
                this.prevText = '';
            }
        } catch (error) {
            console.error('[ShareableMode] Error loading document:', error);
            EditorCore.log(`Error loading document: ${error.message}`, 'error');
            // Initialize document with our stable actor ID
            const baseDoc = Automerge.init({ actorId: this.stableActorId });
            this.doc = Automerge.change(baseDoc, doc => {
                doc.title = "Untitled Document";
                doc.text = "";
                doc.chat_history = [];
            });
            this.currentDocumentTitle = 'Untitled Document';
            this.prevText = '';
        }

        // Get actor ID
        const actorId = this.getActorId();

        // Load existing screen name mappings from document metadata
        if (this.doc.user_metadata) {
            for (const [actId, metadata] of Object.entries(this.doc.user_metadata)) {
                if (metadata.screenName) {
                    this.actorToScreenName[actId] = metadata.screenName;
                }
            }
        }

        // Store screen name in document metadata
        // Server has already validated uniqueness during registration
        const oldDoc = this.doc;
        const newDoc = Automerge.change(oldDoc, doc => {
            if (!doc.user_metadata) {
                doc.user_metadata = {};
            }
            if (!doc.user_metadata[actorId] || doc.user_metadata[actorId].screenName !== this.screenName) {
                doc.user_metadata[actorId] = {
                    screenName: this.screenName,
                    lastSeen: Date.now()
                };
            }
        });

        // Get the changes
        const metadataChanges = Automerge.getChanges(oldDoc, newDoc);

        // Update local document
        this.doc = newDoc;

        // Send metadata changes to server if any
        if (metadataChanges.length > 0) {
            await this.applyChangesToServer(docKey, metadataChanges);
        }

        // Update our local mapping with our own screen name
        this.actorToScreenName[actorId] = this.screenName;

        // Set up WebSocket for changes
        this.socket = EditorCore.setupWebSocket(
            actorId,
            docKey,
            (channel, messageData) => this.handleIncomingMessage(messageData),
            (command) => this.handleKeyspaceNotification(docKey, command)
        );

        this.currentDocument = documentId;

        // Announce presence immediately
        await this.announcePresence(documentId);

        // Start heartbeat to maintain presence
        this.startHeartbeat(documentId);

        // Update UI
        document.getElementById('current-document-name').textContent = this.currentDocumentTitle;
        document.getElementById('sync-status-shareable-editor').textContent = 'Syncing ✓';

        // Set up title input
        const titleInput = document.getElementById('document-title-input');
        titleInput.value = this.currentDocumentTitle;
        titleInput.disabled = false;

        // Add title change listener (debounced)
        const titleChangeHandler = EditorCore.debounce(() => this.handleTitleChange(), 500);
        titleInput.removeEventListener('input', this._titleHandler);
        this._titleHandler = titleChangeHandler;
        titleInput.addEventListener('input', titleChangeHandler);

        // Set up editor event listener (remove old listener first to avoid duplicates)
        const editor = document.getElementById('editor-shareable');
        const newHandler = EditorCore.debounce(() => this.handleEdit(), 300);
        editor.removeEventListener('input', this._currentHandler);
        this._currentHandler = newHandler;
        editor.addEventListener('input', newHandler);

        // Update editor when it loses focus (remove old listener first to avoid duplicates)
        const newBlurHandler = () => {
            EditorCore.log('Editor lost focus - syncing any pending changes', 'shareable');
            this.updateEditor();
        };
        if (this._blurHandler) {
            editor.removeEventListener('blur', this._blurHandler);
        }
        this._blurHandler = newBlurHandler;
        editor.addEventListener('blur', newBlurHandler);

        // Update the editor with loaded content
        const textarea = document.getElementById('editor-shareable');
        // Convert Automerge Text object to plain string
        const textToDisplay = this.doc.text ? this.doc.text.toString() : '';
        textarea.value = textToDisplay;
        this.prevText = textToDisplay;

        // Update line numbers
        this.updateLineNumbers();

        // Set up scroll synchronization for line numbers
        textarea.addEventListener('scroll', () => this.syncLineNumbersScroll());

        EditorCore.log(`Connected to document: ${this.currentDocumentTitle} (${documentId})`, 'shareable');

        // Update URL without reloading
        const url = new URL(window.location);
        url.searchParams.set('document', documentId);
        window.history.pushState({}, '', url);

        // Load and display history
        await this.loadHistory(docKey);

        // Load and display chat messages (initialize if needed)
        if (!this.doc.chat_history) {
            this.doc.chat_history = [];
        }
        this.displayChatMessages();

        // Set up Enter key listener for chat input
        const chatInput = document.getElementById('chat-input');
        chatInput.addEventListener('keypress', (e) => {
            if (e.key === 'Enter') {
                this.sendChatMessage();
            }
        });

        // Trigger immediate document list refresh to update user count
        await this.triggerDocumentListRefresh();
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
            // Use a different actor ID for history replay (it won't be used for new changes)
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

        listDiv.innerHTML = historyItems.map((item, idx) => {
            const actorName = this.getScreenName(item.actor.toString());
            const itemId = `history-item-${idx}`;

            // Format change details for collapsed view
            let detailsHtml = '';
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

                // Extract text from description (remove "Inserted: ", "Deleted: " prefixes)
                let text = '';
                if (details.type === 'insert') {
                    text = details.description.replace('Inserted "', '').replace('"', '');
                } else if (details.type === 'delete') {
                    text = details.description.replace('Deleted "', '').replace('"', '');
                } else if (details.type === 'replace') {
                    text = details.description.replace('Replaced "', '').replace('"', '');
                } else {
                    text = details.description;
                }

                detailsHtml = `
                    <div class="history-details" id="${itemId}-details" style="display: none; margin-top: 6px; padding: 6px 8px; background: #f9fafb; border-radius: 4px; font-size: 11px;">
                        <div style="color: ${typeColor}; font-weight: 600; margin-bottom: 4px;">
                            ${typeIcon} ${details.type.toUpperCase()}: ${text}
                        </div>
                        ${details.added > 0 || details.removed > 0 ? `
                            <div style="font-size: 10px; color: #999;">
                                ${details.added > 0 ? `<span style="color: #10b981;">+${details.added}</span>` : ''}
                                ${details.removed > 0 ? `<span style="color: #ef4444;"> -${details.removed}</span>` : ''}
                                ${details.position !== undefined ? ` @ pos ${details.position}` : ''}
                            </div>
                        ` : ''}
                    </div>
                `;
            }

            return `
                <div class="history-item" style="padding: 6px 10px; border-bottom: 1px solid #e5e7eb; cursor: pointer; user-select: none;" onclick="ShareableMode.toggleHistoryItem('${itemId}')">
                    <div style="display: flex; align-items: center; gap: 6px;">
                        <span id="${itemId}-arrow" style="font-size: 10px; color: #9ca3af;">▶</span>
                        <span style="font-weight: 600; font-size: 13px;">#${item.index}: ${actorName}</span>
                    </div>
                    ${detailsHtml}
                </div>
            `;
        }).join('');
    },

    /**
     * Toggle history item details
     * @param {string} itemId - The history item ID
     */
    toggleHistoryItem(itemId) {
        const details = document.getElementById(`${itemId}-details`);
        const arrow = document.getElementById(`${itemId}-arrow`);

        if (details && arrow) {
            if (details.style.display === 'none') {
                details.style.display = 'block';
                arrow.textContent = '▼';
            } else {
                details.style.display = 'none';
                arrow.textContent = '▶';
            }
        }
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

        const actorName = this.getScreenName(item.actor.toString());
        const itemId = `history-item-${Date.now()}`;

        // Format change details for collapsed view
        let detailsHtml = '';
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

            // Extract text from description (remove "Inserted: ", "Deleted: " prefixes)
            let text = '';
            if (details.type === 'insert') {
                text = details.description.replace('Inserted "', '').replace('"', '');
            } else if (details.type === 'delete') {
                text = details.description.replace('Deleted "', '').replace('"', '');
            } else if (details.type === 'replace') {
                text = details.description.replace('Replaced "', '').replace('"', '');
            } else {
                text = details.description;
            }

            detailsHtml = `
                <div class="history-details" id="${itemId}-details" style="display: none; margin-top: 6px; padding: 6px 8px; background: #f9fafb; border-radius: 4px; font-size: 11px;">
                    <div style="color: ${typeColor}; font-weight: 600; margin-bottom: 4px;">
                        ${typeIcon} ${details.type.toUpperCase()}: ${text}
                    </div>
                    ${details.added > 0 || details.removed > 0 ? `
                        <div style="font-size: 10px; color: #999;">
                            ${details.added > 0 ? `<span style="color: #10b981;">+${details.added}</span>` : ''}
                            ${details.removed > 0 ? `<span style="color: #ef4444;"> -${details.removed}</span>` : ''}
                            ${details.position !== undefined ? ` @ pos ${details.position}` : ''}
                        </div>
                    ` : ''}
                </div>
            `;
        }

        const itemHtml = `
            <div class="history-item" style="padding: 6px 10px; border-bottom: 1px solid #e5e7eb; cursor: pointer; user-select: none;" onclick="ShareableMode.toggleHistoryItem('${itemId}')">
                <div style="display: flex; align-items: center; gap: 6px;">
                    <span id="${itemId}-arrow" style="font-size: 10px; color: #9ca3af;">▶</span>
                    <span style="font-weight: 600; font-size: 13px;">#${item.index}: ${actorName}</span>
                </div>
                ${detailsHtml}
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
            // Get old text, title, and chat history before applying change
            const oldText = this.doc.text ? this.doc.text.toString() : '';
            const oldTitle = this.doc.title ? this.doc.title.toString() : '';
            const oldChatLength = this.doc.chat_history ? this.doc.chat_history.length : 0;

            const changeBytes = Uint8Array.from(atob(messageData), c => c.charCodeAt(0));
            const [newDoc] = Automerge.applyChanges(this.doc, [changeBytes]);

            // Get new text, title, and chat history after applying change
            const newText = newDoc.text ? newDoc.text.toString() : '';
            const newTitle = newDoc.title ? newDoc.title.toString() : '';
            const newChatLength = newDoc.chat_history ? newDoc.chat_history.length : 0;

            this.doc = newDoc;

            // Update title if it changed
            if (newTitle !== oldTitle) {
                this.currentDocumentTitle = newTitle;
                document.getElementById('current-document-name').textContent = newTitle;
                const titleInput = document.getElementById('document-title-input');
                // Only update if user is not currently editing the title
                if (document.activeElement !== titleInput) {
                    titleInput.value = newTitle;
                }
                EditorCore.log(`Document title updated to: "${newTitle}"`, 'shareable');
            }

            // Update screen name mappings from metadata
            if (this.doc.user_metadata) {
                for (const [actId, metadata] of Object.entries(this.doc.user_metadata)) {
                    if (metadata.screenName) {
                        this.actorToScreenName[actId] = metadata.screenName;
                    }
                }
            }

            this.updateEditor();
            this.updateDocInfo();

            // Check if chat history changed
            if (newChatLength > oldChatLength) {
                // New chat message received
                this.displayChatMessages();
                EditorCore.log(`New chat message received`, 'shareable');
            }

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
        if (!this.currentDocument) return;

        const docKey = `am:document:${this.currentDocument}`;
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

        // Apply change locally using Automerge
        const oldDoc = this.doc;
        const newDoc = Automerge.change(oldDoc, doc => {
            // In Automerge 3.x, text fields are plain strings
            // Convert Text object to string if needed (for backwards compatibility)
            const oldText = doc.text ? doc.text.toString() : '';
            const newText = oldText.substring(0, splice.pos) +
                           splice.text +
                           oldText.substring(splice.pos + splice.del);
            doc.text = newText;
        });

        // Get the changes that were just created
        const changes = Automerge.getChanges(oldDoc, newDoc);

        // Update local document
        this.doc = newDoc;

        // Send changes to server via AM.APPLY
        await this.applyChangesToServer(docKey, changes);

        // Update prevText
        this.prevText = newText;

        // Update local UI
        this.updateDocInfo();

        // Update line numbers after local edit
        this.updateLineNumbers();

        // Update history sidebar with the local change
        const changeDetails = this.calculateChangeDetails(oldText, newText);
        const history = Automerge.getHistory(this.doc);
        const latestChange = history[history.length - 1];

        this.prependHistoryItem({
            index: history.length,
            timestamp: latestChange.change.time || Date.now(),
            actor: latestChange.change.actor || 'unknown',
            message: latestChange.change.message || 'Document change',
            changeDetails: changeDetails
        });

        // Clear editing flag after a short delay (to handle echo-back)
        setTimeout(() => {
            this.isLocallyEditing = false;
        }, 100);
    },

    /**
     * Handle title changes from the title input
     */
    async handleTitleChange() {
        if (!this.currentDocument) return;

        const titleInput = document.getElementById('document-title-input');
        const newTitle = titleInput.value.trim();

        // Don't update if title hasn't actually changed
        if (newTitle === this.currentDocumentTitle) {
            return;
        }

        // Don't allow empty titles
        if (!newTitle) {
            titleInput.value = this.currentDocumentTitle;
            return;
        }

        const docKey = `am:document:${this.currentDocument}`;

        EditorCore.log(`Updating document title to: "${newTitle}"`, 'shareable');

        // Apply change locally using Automerge
        const oldDoc = this.doc;
        const newDoc = Automerge.change(oldDoc, doc => {
            doc.title = newTitle;
        });

        // Get the changes that were just created
        const changes = Automerge.getChanges(oldDoc, newDoc);

        // Update local document
        this.doc = newDoc;
        this.currentDocumentTitle = newTitle;

        // Send changes to server via AM.APPLY
        await this.applyChangesToServer(docKey, changes);

        // Update UI
        document.getElementById('current-document-name').textContent = newTitle;

        EditorCore.log(`Document title updated successfully`, 'shareable');
    },

    /**
     * Apply Automerge changes to the server
     * @param {string} docKey - The document key
     * @param {Array} changes - Array of Automerge change objects
     */
    async applyChangesToServer(docKey, changes) {
        try {
            for (const change of changes) {
                EditorCore.log(`Sending change to server (${change.length} bytes)`, 'shareable');

                // Send raw binary change to server via AM.APPLY
                // The Uint8Array is sent directly as binary data
                const response = await fetch(`${EditorCore.WEBDIS_URL}/AM.APPLY/${docKey}`, {
                    method: 'PUT',
                    headers: {
                        'Content-Type': 'application/octet-stream'
                    },
                    body: change  // Send Uint8Array directly
                });

                const data = await response.json();
                EditorCore.log(`Server response: ${JSON.stringify(data)}`, 'shareable');

                if (data['AM.APPLY'] && (data['AM.APPLY'] === 'OK' || data['AM.APPLY'][0] === true)) {
                    EditorCore.log(`Change applied successfully to server`, 'shareable');
                } else {
                    EditorCore.log(`AM.APPLY error: ${JSON.stringify(data)}`, 'error');
                }
            }
        } catch (error) {
            EditorCore.log(`Error applying changes to server: ${error.message}`, 'error');
            console.error('Server apply error:', error);
        }
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

            // Update line numbers after content change
            this.updateLineNumbers();
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
     * Update line numbers based on textarea content
     */
    updateLineNumbers() {
        const textarea = document.getElementById('editor-shareable');
        const lineNumbersDiv = document.getElementById('line-numbers');

        if (!textarea || !lineNumbersDiv) return;

        const text = textarea.value;
        const lines = text.split('\n');
        const lineCount = lines.length;

        // Generate line numbers
        const lineNumbers = Array.from({ length: lineCount }, (_, i) => i + 1).join('\n');
        lineNumbersDiv.textContent = lineNumbers;
    },

    /**
     * Sync line numbers scroll with textarea scroll
     */
    syncLineNumbersScroll() {
        const textarea = document.getElementById('editor-shareable');
        const lineNumbersDiv = document.getElementById('line-numbers');

        if (!textarea || !lineNumbersDiv) return;

        lineNumbersDiv.scrollTop = textarea.scrollTop;
    },

    /**
     * Disconnect from current document
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
        this.currentDocument = null;
        this.currentDocumentTitle = null;
        this.prevText = '';

        // Update UI
        document.getElementById('current-document-name').textContent = 'Not connected';
        document.getElementById('editor-shareable').value = '';
        document.getElementById('sync-status-shareable-editor').textContent = 'Not syncing';

        // Disable and clear title input
        const titleInput = document.getElementById('document-title-input');
        titleInput.value = '';
        titleInput.disabled = true;

        // Remove document from URL
        const url = new URL(window.location);
        url.searchParams.delete('document');
        window.history.pushState({}, '', url);

        EditorCore.log('Disconnected from document', 'shareable');

        // Trigger immediate document list refresh to update user count
        await this.triggerDocumentListRefresh();
    },

    /**
     * Copy shareable link to clipboard
     */
    async copyShareableLink() {
        if (!this.currentDocument) return;

        const url = new URL(window.location);
        url.searchParams.set('document', this.currentDocument);
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
     * Delete the current document with confirmation
     */
    async deleteDocument() {
        if (!this.currentDocument) {
            alert('No document is currently open');
            return;
        }

        const documentTitle = this.currentDocumentTitle || this.currentDocument;
        const documentId = this.currentDocument;

        // Show confirmation dialog
        const confirmed = confirm(
            `Are you sure you want to delete "${documentTitle}"?\n\n` +
            `This action cannot be undone and will permanently remove the document ` +
            `and all its history.`
        );

        if (!confirmed) {
            EditorCore.log('Document deletion cancelled', 'shareable');
            return;
        }

        const docKey = `am:document:${documentId}`;

        EditorCore.log(`Deleting document: ${documentTitle} (${documentId})`, 'shareable');

        try {
            // Delete the document from Redis
            const response = await fetch(`${EditorCore.WEBDIS_URL}/DEL/${docKey}`);
            const data = await response.json();

            if (data.DEL === 1 || data.DEL === true) {
                EditorCore.log(`Document deleted successfully: ${documentTitle}`, 'shareable');

                // Disconnect from the document
                await this.disconnect();

                // Refresh document list
                await this.refreshDocumentList();

                alert(`Document "${documentTitle}" has been deleted.`);
            } else {
                EditorCore.log(`Failed to delete document: ${JSON.stringify(data)}`, 'error');
                alert('Failed to delete document. It may not exist or may have already been deleted.');
            }
        } catch (error) {
            EditorCore.log(`Error deleting document: ${error.message}`, 'error');
            console.error('Delete error:', error);
            alert('An error occurred while deleting the document. Please try again.');
        }
    },

    /**
     * Refresh the document list
     */
    async refreshDocumentList() {
        EditorCore.log('Refreshing document list', 'shareable');

        try {
            // Use KEYS to find all documents (for demo; use SCAN in production)
            const response = await fetch(`${EditorCore.WEBDIS_URL}/KEYS/am:document:*`);
            const data = await response.json();

            const documentIds = (data.KEYS || []).map(key => {
                // Extract document ID (UUID) from key
                return key.replace('am:document:', '');
            });

            // Get active user counts, peer lists, and titles
            const documentsWithUsers = await Promise.all(
                documentIds.map(async (documentId) => {
                    // Channel format: changes:am:document:{documentId}
                    const docKey = `am:document:${documentId}`;
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

                    // Get peer list for this document
                    const peers = await this.getPeersInDocument(documentId);

                    // Track peer changes to update activity timestamp
                    const previousPeers = this.documentPeers[documentId] || new Set();
                    const currentPeersSet = new Set(peers);

                    // Check if peers changed (joined or left)
                    const peersChanged =
                        previousPeers.size !== currentPeersSet.size ||
                        [...previousPeers].some(p => !currentPeersSet.has(p));

                    if (peersChanged) {
                        this.documentActivity[documentId] = Date.now();
                        this.documentPeers[documentId] = currentPeersSet;
                    }

                    // Ensure document has an activity timestamp (default to 0 for new documents)
                    if (!this.documentActivity[documentId]) {
                        this.documentActivity[documentId] = 0;
                    }

                    // Load document title (use AM.GETTEXT to fetch just the title field)
                    let title = 'Untitled Document';
                    try {
                        const titleResponse = await fetch(`${EditorCore.WEBDIS_URL}/AM.GETTEXT/${docKey}/title`);
                        const titleData = await titleResponse.json();
                        if (titleData['AM.GETTEXT']) {
                            title = titleData['AM.GETTEXT'];
                        }
                    } catch (error) {
                        EditorCore.log(`Error loading title for ${documentId}: ${error.message}`, 'error');
                    }

                    return {
                        documentId,
                        title,
                        activeUsers,
                        peers,
                        lastActivity: this.documentActivity[documentId]
                    };
                })
            );

            // Sort by most recent activity (newest first)
            this.documentList = documentsWithUsers.sort((a, b) => b.lastActivity - a.lastActivity);
            this.updateDocumentListUI();

        } catch (error) {
            EditorCore.log(`Error refreshing document list: ${error.message}`, 'error');
        }
    },

    /**
     * Update the document list UI
     */
    updateDocumentListUI() {
        const listDiv = document.getElementById('document-list');

        if (this.documentList.length === 0) {
            listDiv.innerHTML = '<div style="padding: 20px; text-align: center; color: #999;">No documents available. Create one to get started!</div>';
            return;
        }

        const currentActorId = this.getActorId();

        listDiv.innerHTML = this.documentList.map(({ documentId, title, activeUsers, peers }) => {
            const userClass = activeUsers > 0 ? 'active' : '';
            const userText = activeUsers === 1 ? '1 user' : `${activeUsers} users`;

            // Show peers if this is the current document
            const isCurrentDocument = this.currentDocument === documentId;
            const peerListHtml = isCurrentDocument && peers && peers.length > 0
                ? `<div class="peer-list">${peers.map(peer => {
                    // Check if current user by comparing screenName
                    const isCurrentUser = peer.screenName === this.screenName;
                    const peerClass = isCurrentUser ? 'peer-item peer-item-current' : 'peer-item';

                    // Display name with replica count if multiple
                    const displayName = peer.actorIds.length > 1
                        ? `${peer.screenName} (${peer.actorIds.length})`
                        : peer.screenName;

                    // Tooltip shows all actorIds for this user
                    const actorIdList = peer.actorIds.map(id => id.slice(0, 8) + '...').join(', ');
                    const tooltip = peer.actorIds.length > 1
                        ? `${peer.actorIds.length} replicas: ${actorIdList}`
                        : `Actor ID: ${peer.actorIds[0]}`;

                    return `<div class="${peerClass}" title="${tooltip}">${displayName}</div>`;
                  }).join('')}</div>`
                : '';

            return `
                <div class="document-item-container ${isCurrentDocument ? 'current-document' : ''}">
                    <div class="document-item" onclick="ShareableMode.joinDocument('${documentId}')">
                        <span class="document-name">${this.escapeHtml(title)}</span>
                        <span class="document-users ${userClass}">${userText}</span>
                    </div>
                    ${peerListHtml}
                </div>
            `;
        }).join('');
    },

    /**
     * Subscribe to document creation/deletion events
     */
    subscribeToDocumentEvents() {
        const pattern = '__keyspace@0__:am:document:*';

        this.documentListSocket = new WebSocket(`${EditorCore.WEBDIS_WS_URL}/.json`);

        this.documentListSocket.onopen = () => {
            EditorCore.log('Document list WebSocket connected', 'shareable');
            this.documentListSocket.send(JSON.stringify(['PSUBSCRIBE', pattern]));
        };

        this.documentListSocket.onmessage = async (event) => {
            try {
                const response = JSON.parse(event.data);

                if (response.PSUBSCRIBE || response.PMESSAGE) {
                    const data = response.PSUBSCRIBE || response.PMESSAGE;

                    if (data[0] === 'psubscribe') {
                        EditorCore.log(`Subscribed to pattern: ${data[1]}`, 'shareable');
                    } else if (data[0] === 'pmessage') {
                        const command = data[3];
                        EditorCore.log(`Document event detected: ${command}`, 'shareable');
                        // Refresh document list when documents are created or deleted
                        if (command === 'am.new' || command === 'del') {
                            await this.refreshDocumentList();
                        }
                    }
                }
            } catch (error) {
                EditorCore.log(`Document list WebSocket error: ${error.message}`, 'error');
            }
        };

        this.documentListSocket.onerror = (error) => {
            EditorCore.log('Document list WebSocket error', 'error');
        };

        this.documentListSocket.onclose = () => {
            EditorCore.log('Document list WebSocket closed', 'shareable');
        };
    },

    /**
     * Start periodic document list refresh with smart debouncing
     */
    startDocumentListRefresh() {
        let lastRefresh = Date.now();
        const minInterval = 2000; // Minimum 2 seconds between refreshes

        // Refresh every 5 seconds normally
        this.refreshInterval = setInterval(async () => {
            const now = Date.now();
            if (now - lastRefresh >= minInterval) {
                await this.refreshDocumentList();
                lastRefresh = now;
            }
        }, 5000);
    },

    /**
     * Trigger an immediate document list refresh (debounced)
     */
    async triggerDocumentListRefresh() {
        // Debounce rapid calls
        clearTimeout(this._refreshDebounce);
        this._refreshDebounce = setTimeout(async () => {
            await this.refreshDocumentList();
        }, 500);
    },

    /**
     * Announce presence in a document
     */
    async announcePresence(documentId) {
        try {
            const actorId = this.getActorId();
            const presenceKey = `presence:${documentId}`;
            // Store screenName:actorId to enable user-level deduplication
            // (ActorIds represent CRDT replicas, not users - same user can have multiple replicas)
            const presenceValue = `${this.screenName}:${actorId}`;
            await fetch(`${EditorCore.WEBDIS_URL}/SETEX/${presenceKey}:${actorId}/10/${presenceValue}`);
            EditorCore.log(`Announced presence in document ${documentId}`, 'shareable');
        } catch (error) {
            EditorCore.log(`Error announcing presence: ${error.message}`, 'error');
        }
    },

    /**
     * Start heartbeat to maintain presence
     */
    startHeartbeat(documentId) {
        // Announce presence every 5 seconds
        this.heartbeatInterval = setInterval(async () => {
            if (this.currentDocument === documentId) {
                await this.announcePresence(documentId);
            }
        }, 5000);
    },

    /**
     * Get peers in a document, grouped by user (screenName)
     * @returns {Promise<Array<{screenName: string, actorIds: string[]}>>}
     */
    async getPeersInDocument(documentId) {
        try {
            const pattern = `presence:${documentId}:*`;
            const response = await fetch(`${EditorCore.WEBDIS_URL}/KEYS/${pattern}`);
            const data = await response.json();

            const keys = data.KEYS || [];

            // Fetch values for all presence keys (values are "screenName:actorId")
            const presenceData = await Promise.all(
                keys.map(async (key) => {
                    const getResponse = await fetch(`${EditorCore.WEBDIS_URL}/GET/${key}`);
                    const getData = await getResponse.json();
                    return getData.GET;
                })
            );

            // Parse and group by screenName
            const userMap = {}; // { screenName: [actorId1, actorId2, ...] }

            for (const value of presenceData) {
                if (!value) continue;

                // Parse "screenName:actorId" format
                const parts = value.split(':');
                if (parts.length >= 2) {
                    const screenName = parts[0];
                    const actorId = parts.slice(1).join(':'); // Handle actorIds with colons

                    if (!userMap[screenName]) {
                        userMap[screenName] = [];
                    }
                    userMap[screenName].push(actorId);
                }
            }

            // Convert to array of {screenName, actorIds[]}
            return Object.entries(userMap).map(([screenName, actorIds]) => ({
                screenName,
                actorIds: [...new Set(actorIds)] // Deduplicate actorIds
            }));
        } catch (error) {
            EditorCore.log(`Error getting peers: ${error.message}`, 'error');
            return [];
        }
    },

    /**
     * Send a chat message
     */
    async sendChatMessage() {
        if (!this.currentDocument) return;

        const chatInput = document.getElementById('chat-input');
        const messageText = chatInput.value.trim();

        if (!messageText) return;

        const docKey = `am:document:${this.currentDocument}`;
        const actorId = this.getActorId();

        EditorCore.log(`Sending chat message: "${messageText}"`, 'shareable');

        // Apply change locally using Automerge
        const oldDoc = this.doc;
        const newDoc = Automerge.change(oldDoc, doc => {
            if (!doc.chat_history) {
                doc.chat_history = [];
            }
            doc.chat_history.push({
                actor: actorId,
                message: messageText,
                timestamp: Date.now()
            });
        });

        // Get the changes that were just created
        const changes = Automerge.getChanges(oldDoc, newDoc);

        // Update local document
        this.doc = newDoc;

        // Send changes to server via AM.APPLY
        await this.applyChangesToServer(docKey, changes);

        // Clear input
        chatInput.value = '';

        // Update chat display
        this.displayChatMessages();

        EditorCore.log(`Chat message sent`, 'shareable');
    },

    /**
     * Display chat messages from the document
     */
    displayChatMessages() {
        const chatListDiv = document.getElementById('chat-list');
        const chatContainerDiv = chatListDiv.parentElement; // The actual scrolling container

        if (!this.doc || !this.doc.chat_history || this.doc.chat_history.length === 0) {
            chatListDiv.innerHTML = '<div style="padding: 20px; text-align: center; color: #999;">No messages yet</div>';
            this._displayedChatCount = 0;
            return;
        }

        const currentChatLength = this.doc.chat_history.length;

        // Check if we need to do a full refresh or just append new messages
        if (this._displayedChatCount === 0 || this._displayedChatCount > currentChatLength) {
            // Full refresh needed (first load or chat was cleared)
            chatListDiv.innerHTML = this.doc.chat_history.map((msg, index) => {
                const time = new Date(msg.timestamp).toLocaleTimeString();
                const actorName = msg.actor ? this.getScreenName(msg.actor.toString()) : 'unknown';
                // Compare by screenName, not actorId (same user can have multiple replicas)
                const isCurrentUser = actorName === this.screenName;
                const messageClass = isCurrentUser ? 'chat-message chat-message-current' : 'chat-message';
                const actorClass = isCurrentUser ? 'chat-message-actor chat-message-actor-current' : 'chat-message-actor';

                return `
                    <div class="${messageClass}">
                        <div class="chat-message-header">
                            <span class="${actorClass}">${actorName}</span>
                            <span class="chat-message-time">${time}</span>
                        </div>
                        <div class="chat-message-text">${this.escapeHtml(msg.message)}</div>
                    </div>
                `;
            }).join('');

            this._displayedChatCount = currentChatLength;

            // Scroll to bottom on initial load
            requestAnimationFrame(() => {
                chatContainerDiv.scrollTop = chatContainerDiv.scrollHeight;
            });
        } else if (currentChatLength > this._displayedChatCount) {
            // Append only new messages
            const newMessages = this.doc.chat_history.slice(this._displayedChatCount);

            // Check if user is at or near the bottom before adding new messages
            const oldScrollHeight = chatContainerDiv.scrollHeight;
            const oldScrollTop = chatContainerDiv.scrollTop;
            const clientHeight = chatContainerDiv.clientHeight;
            const distanceFromBottom = oldScrollHeight - oldScrollTop - clientHeight;
            const shouldScrollToBottom = distanceFromBottom < 50; // User is following the conversation

            // Append new messages
            const newMessagesHtml = newMessages.map((msg, index) => {
                const time = new Date(msg.timestamp).toLocaleTimeString();
                const actorName = msg.actor ? this.getScreenName(msg.actor.toString()) : 'unknown';
                // Compare by screenName, not actorId (same user can have multiple replicas)
                const isCurrentUser = actorName === this.screenName;
                const messageClass = isCurrentUser ? 'chat-message chat-message-current' : 'chat-message';
                const actorClass = isCurrentUser ? 'chat-message-actor chat-message-actor-current' : 'chat-message-actor';

                return `
                    <div class="${messageClass}">
                        <div class="chat-message-header">
                            <span class="${actorClass}">${actorName}</span>
                            <span class="chat-message-time">${time}</span>
                        </div>
                        <div class="chat-message-text">${this.escapeHtml(msg.message)}</div>
                    </div>
                `;
            }).join('');

            chatListDiv.insertAdjacentHTML('beforeend', newMessagesHtml);

            this._displayedChatCount = currentChatLength;

            // Smoothly scroll to bottom if user was at the bottom
            if (shouldScrollToBottom) {
                // Use requestAnimationFrame to ensure DOM has fully updated
                requestAnimationFrame(() => {
                    requestAnimationFrame(() => {
                        chatContainerDiv.scrollTop = chatContainerDiv.scrollHeight;
                    });
                });
            }
        }
    },

    /**
     * Escape HTML to prevent XSS
     */
    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
};

// Make ShareableMode globally available
window.ShareableMode = ShareableMode;

// Utility functions for sync log
function log(message, source = 'server') {
    EditorCore.log(message, source);
}

function clearLog() {
    document.getElementById('sync-log').innerHTML = '';
}

function selectAllLog() {
    const logDiv = document.getElementById('sync-log');
    const range = document.createRange();
    range.selectNodeContents(logDiv);
    const selection = window.getSelection();
    selection.removeAllRanges();
    selection.addRange(range);
}

function toggleSyncLog() {
    const logDiv = document.getElementById('sync-log');
    const buttonsDiv = document.getElementById('sync-log-buttons');
    const toggleSpan = document.getElementById('sync-log-toggle');

    if (logDiv.style.display === 'none') {
        logDiv.style.display = 'block';
        buttonsDiv.style.display = 'block';
        toggleSpan.textContent = '▼';
    } else {
        logDiv.style.display = 'none';
        buttonsDiv.style.display = 'none';
        toggleSpan.textContent = '▶';
    }
}

// Initialize
window.addEventListener('load', async () => {
    EditorCore.log('Collaborative editor loaded', 'server');

    // Check if Automerge loaded
    if (typeof Automerge === 'undefined') {
        EditorCore.log('ERROR: Automerge library failed to load!', 'error');
    } else {
        EditorCore.log('Automerge library ready', 'server');
    }

    // Check connection
    const connected = await EditorCore.checkConnection();
    if (connected) {
        const statusElement = document.getElementById('connection-status-shareable-editor');
        if (statusElement) {
            statusElement.textContent = 'Connected';
            statusElement.className = 'status-connected';
        }
    }

    // Initialize ShareableMode
    await ShareableMode.initialize();
});
