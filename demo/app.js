// Webdis connection
const WEBDIS_URL = 'http://localhost:7379';

// Output logging
function log(message, type = 'info') {
    const output = document.getElementById('output');
    const timestamp = new Date().toLocaleTimeString();
    const className = `log-${type}`;
    output.innerHTML += `<span class="${className}">[${timestamp}] ${message}</span>\n`;
    output.scrollTop = output.scrollHeight;
}

function logCommand(command) {
    log(`> ${command}`, 'command');
}

function clearOutput() {
    document.getElementById('output').innerHTML = '';
}

// Helper function to make Redis commands via Webdis
async function redisCommand(...args) {
    const command = args.join('/');
    const url = `${WEBDIS_URL}/${command}`;

    logCommand(args.join(' '));

    try {
        const response = await fetch(url);
        const data = await response.json();

        if (data.error) {
            log(`Error: ${data.error}`, 'error');
            return null;
        }

        // Handle different response types
        let result = data;
        if (typeof data === 'object' && data !== null) {
            // Webdis wraps responses in objects with type info
            if ('REPLY' in data) {
                result = data.REPLY;
            } else if (Object.keys(data).length === 1) {
                result = Object.values(data)[0];
            }
        }

        log(`✓ ${JSON.stringify(result)}`, 'success');
        return result;
    } catch (error) {
        log(`Network error: ${error.message}`, 'error');
        updateConnectionStatus(false);
        return null;
    }
}

// Connection status
async function checkConnection() {
    try {
        const response = await fetch(`${WEBDIS_URL}/PING`);
        const data = await response.json();
        updateConnectionStatus(data.PING === 'PONG' || data.PING === true);
    } catch (error) {
        updateConnectionStatus(false);
    }
}

function updateConnectionStatus(connected) {
    const status = document.getElementById('connection-status');
    if (connected) {
        status.textContent = 'Connected';
        status.className = 'status-connected';
    } else {
        status.textContent = 'Disconnected';
        status.className = 'status-disconnected';
    }
}

// Document operations
async function createDocument() {
    const key = document.getElementById('doc-key').value;
    if (!key) {
        log('Please enter a document key', 'error');
        return;
    }
    await redisCommand('AM.NEW', key);
}

async function deleteDocument() {
    const key = document.getElementById('doc-key').value;
    if (!key) {
        log('Please enter a document key', 'error');
        return;
    }
    await redisCommand('DEL', key);
}

// Text operations
async function putText() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('text-path').value;
    const value = document.getElementById('text-value').value;

    if (!key || !path || !value) {
        log('Please fill in all text fields', 'error');
        return;
    }

    await redisCommand('AM.PUTTEXT', key, path, value);
}

async function getText() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('text-path').value;

    if (!key || !path) {
        log('Please enter document key and path', 'error');
        return;
    }

    await redisCommand('AM.GETTEXT', key, path);
}

// Integer operations
async function putInt() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('int-path').value;
    const value = document.getElementById('int-value').value;

    if (!key || !path || value === '') {
        log('Please fill in all integer fields', 'error');
        return;
    }

    await redisCommand('AM.PUTINT', key, path, value);
}

async function getInt() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('int-path').value;

    if (!key || !path) {
        log('Please enter document key and path', 'error');
        return;
    }

    await redisCommand('AM.GETINT', key, path);
}

// Double operations
async function putDouble() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('double-path').value;
    const value = document.getElementById('double-value').value;

    if (!key || !path || value === '') {
        log('Please fill in all double fields', 'error');
        return;
    }

    await redisCommand('AM.PUTDOUBLE', key, path, value);
}

async function getDouble() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('double-path').value;

    if (!key || !path) {
        log('Please enter document key and path', 'error');
        return;
    }

    await redisCommand('AM.GETDOUBLE', key, path);
}

// Boolean operations
async function putBool() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('bool-path').value;
    const value = document.getElementById('bool-value').value;

    if (!key || !path) {
        log('Please fill in all boolean fields', 'error');
        return;
    }

    await redisCommand('AM.PUTBOOL', key, path, value);
}

async function getBool() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('bool-path').value;

    if (!key || !path) {
        log('Please enter document key and path', 'error');
        return;
    }

    await redisCommand('AM.GETBOOL', key, path);
}

// List operations
async function createList() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('list-path').value;

    if (!key || !path) {
        log('Please enter document key and list path', 'error');
        return;
    }

    await redisCommand('AM.CREATELIST', key, path);
}

async function listLen() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('list-path').value;

    if (!key || !path) {
        log('Please enter document key and list path', 'error');
        return;
    }

    await redisCommand('AM.LISTLEN', key, path);
}

async function appendToList() {
    const key = document.getElementById('doc-key').value;
    const path = document.getElementById('list-append-path').value;
    const value = document.getElementById('list-append-value').value;
    const type = document.getElementById('list-append-type').value;

    if (!key || !path || !value) {
        log('Please fill in all append fields', 'error');
        return;
    }

    const commands = {
        'text': 'AM.APPENDTEXT',
        'int': 'AM.APPENDINT',
        'double': 'AM.APPENDDOUBLE',
        'bool': 'AM.APPENDBOOL'
    };

    await redisCommand(commands[type], key, path, value);
}

// Example scenarios
async function runUserProfileExample() {
    clearOutput();
    log('Running User Profile Example...', 'info');

    await redisCommand('AM.NEW', 'user:1001');
    await redisCommand('AM.PUTTEXT', 'user:1001', 'name', 'Alice Smith');
    await redisCommand('AM.PUTINT', 'user:1001', 'age', '28');
    await redisCommand('AM.PUTTEXT', 'user:1001', 'email', 'alice@example.com');
    await redisCommand('AM.PUTBOOL', 'user:1001', 'verified', 'true');
    await redisCommand('AM.PUTTEXT', 'user:1001', 'profile.bio', 'Software Engineer');
    await redisCommand('AM.PUTTEXT', 'user:1001', 'profile.location', 'San Francisco');

    log('Retrieving user data...', 'info');
    await redisCommand('AM.GETTEXT', 'user:1001', 'name');
    await redisCommand('AM.GETINT', 'user:1001', 'age');
    await redisCommand('AM.GETTEXT', 'user:1001', 'profile.location');
}

async function runShoppingCartExample() {
    clearOutput();
    log('Running Shopping Cart Example...', 'info');

    await redisCommand('AM.NEW', 'cart:5001');
    await redisCommand('AM.PUTTEXT', 'cart:5001', 'user_id', 'user:1001');
    await redisCommand('AM.PUTINT', 'cart:5001', 'total', '0');
    await redisCommand('AM.CREATELIST', 'cart:5001', 'items');
    await redisCommand('AM.APPENDTEXT', 'cart:5001', 'items', 'Product A');
    await redisCommand('AM.APPENDTEXT', 'cart:5001', 'items', 'Product B');
    await redisCommand('AM.APPENDTEXT', 'cart:5001', 'items', 'Product C');

    log('Retrieving cart data...', 'info');
    await redisCommand('AM.LISTLEN', 'cart:5001', 'items');
    await redisCommand('AM.GETTEXT', 'cart:5001', 'items[0]');
    await redisCommand('AM.GETTEXT', 'cart:5001', 'items[1]');
}

async function runConfigExample() {
    clearOutput();
    log('Running Configuration Example...', 'info');

    await redisCommand('AM.NEW', 'config:main');
    await redisCommand('AM.PUTTEXT', 'config:main', 'database.host', 'localhost');
    await redisCommand('AM.PUTINT', 'config:main', 'database.port', '5432');
    await redisCommand('AM.PUTTEXT', 'config:main', 'cache.host', 'localhost');
    await redisCommand('AM.PUTINT', 'config:main', 'cache.port', '6379');
    await redisCommand('AM.PUTBOOL', 'config:main', 'cache.enabled', 'true');
    await redisCommand('AM.CREATELIST', 'config:main', 'features');
    await redisCommand('AM.APPENDTEXT', 'config:main', 'features', 'new-ui');
    await redisCommand('AM.APPENDTEXT', 'config:main', 'features', 'api-v2');
    await redisCommand('AM.APPENDTEXT', 'config:main', 'features', 'analytics');

    log('Retrieving config data...', 'info');
    await redisCommand('AM.GETTEXT', 'config:main', 'database.host');
    await redisCommand('AM.GETBOOL', 'config:main', 'cache.enabled');
    await redisCommand('AM.LISTLEN', 'config:main', 'features');
}

// Initialize
window.addEventListener('load', () => {
    log('Redis-Automerge Demo loaded', 'info');
    log('Checking connection...', 'info');
    checkConnection();

    // Check connection periodically
    setInterval(checkConnection, 5000);
});
