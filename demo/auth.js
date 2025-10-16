// ============================================================================
// Auth Module - Server-side user management
// ============================================================================

const Auth = {
    WEBDIS_URL: `${window.location.protocol}//${window.location.host}/api`,

    /**
     * Generate a secure random token
     * @returns {string} 64-character hex token
     */
    generateToken() {
        const bytes = crypto.getRandomValues(new Uint8Array(32));
        return Array.from(bytes)
            .map(b => b.toString(16).padStart(2, '0'))
            .join('');
    },

    /**
     * Generate a stable actor ID
     * @returns {string} 32-character hex actor ID
     */
    generateActorId() {
        const bytes = crypto.getRandomValues(new Uint8Array(16));
        return Array.from(bytes)
            .map(b => b.toString(16).padStart(2, '0'))
            .join('');
    },

    /**
     * Register a new user with the server
     * @param {string} screenName - The desired screen name
     * @returns {Promise<{success: boolean, token?: string, actorId?: string, error?: string}>}
     */
    async register(screenName) {
        try {
            const token = this.generateToken();
            const actorId = this.generateActorId();
            const timestamp = Date.now();

            // Check if screen name is already taken
            const checkResponse = await fetch(
                `${this.WEBDIS_URL}/SCARD/user:name:${encodeURIComponent(screenName)}`
            );
            const checkData = await checkResponse.json();
            const existingUsers = checkData.SCARD || 0;

            if (existingUsers > 0) {
                return {
                    success: false,
                    error: `Screen name "${screenName}" is already in use`
                };
            }

            // Store user data in hash
            const userKey = `user:token:${token}`;
            const hmsetResponse = await fetch(
                `${this.WEBDIS_URL}/HMSET/${userKey}/screenName/${encodeURIComponent(screenName)}/actorId/${actorId}/created/${timestamp}/lastSeen/${timestamp}`
            );
            const hmsetData = await hmsetResponse.json();

            // Webdis returns {"HMSET":[true,"OK"]} or {"HMSET":"OK"}
            const hmsetResult = hmsetData.HMSET;
            const isSuccess = hmsetResult === 'OK' ||
                            hmsetResult === true ||
                            (Array.isArray(hmsetResult) && (hmsetResult[0] === true || hmsetResult[1] === 'OK'));

            if (!isSuccess) {
                console.error('HMSET failed:', hmsetData);
                return {
                    success: false,
                    error: 'Failed to store user data'
                };
            }

            // Add token to screen name set
            const saddResponse = await fetch(
                `${this.WEBDIS_URL}/SADD/user:name:${encodeURIComponent(screenName)}/${token}`
            );
            const saddData = await saddResponse.json();

            // Store actor ID to token mapping
            const actorMapResponse = await fetch(
                `${this.WEBDIS_URL}/SET/user:actor:${actorId}/${token}`
            );
            const actorMapData = await actorMapResponse.json();

            return {
                success: true,
                token,
                actorId,
                screenName
            };
        } catch (error) {
            console.error('Registration error:', error);
            return {
                success: false,
                error: `Registration failed: ${error.message}`
            };
        }
    },

    /**
     * Verify a token and get user data
     * @param {string} token - The auth token
     * @returns {Promise<{valid: boolean, screenName?: string, actorId?: string, error?: string}>}
     */
    async verify(token) {
        try {
            const userKey = `user:token:${token}`;
            const response = await fetch(`${this.WEBDIS_URL}/HGETALL/${userKey}`);
            const data = await response.json();

            const hgetall = data.HGETALL;
            if (!hgetall) {
                return {
                    valid: false,
                    error: 'Invalid or expired token'
                };
            }

            // Webdis can return HGETALL as object {"field":"value",...} or array [field, value, ...]
            let userData;
            if (Array.isArray(hgetall)) {
                // Array format: [field1, value1, field2, value2, ...]
                if (hgetall.length === 0) {
                    return {
                        valid: false,
                        error: 'Invalid or expired token'
                    };
                }
                userData = {};
                for (let i = 0; i < hgetall.length; i += 2) {
                    userData[hgetall[i]] = hgetall[i + 1];
                }
            } else if (typeof hgetall === 'object') {
                // Object format: {"field":"value",...}
                userData = hgetall;
            } else {
                return {
                    valid: false,
                    error: 'Unexpected HGETALL response format'
                };
            }

            if (!userData.screenName || !userData.actorId) {
                return {
                    valid: false,
                    error: 'Incomplete user data'
                };
            }

            // Update last seen timestamp
            await fetch(
                `${this.WEBDIS_URL}/HSET/${userKey}/lastSeen/${Date.now()}`
            );

            return {
                valid: true,
                screenName: userData.screenName,
                actorId: userData.actorId
            };
        } catch (error) {
            console.error('Verification error:', error);
            return {
                valid: false,
                error: `Verification failed: ${error.message}`
            };
        }
    },

    /**
     * Logout - remove user data from server
     * @param {string} token - The auth token
     * @returns {Promise<boolean>} True if successful
     */
    async logout(token) {
        try {
            // Get user data first to get screen name and actor ID
            const userData = await this.verify(token);
            if (!userData.valid) {
                return false;
            }

            const { screenName, actorId } = userData;

            // Remove token from screen name set
            await fetch(
                `${this.WEBDIS_URL}/SREM/user:name:${encodeURIComponent(screenName)}/${token}`
            );

            // Remove actor ID mapping
            await fetch(`${this.WEBDIS_URL}/DEL/user:actor:${actorId}`);

            // Remove user data hash
            await fetch(`${this.WEBDIS_URL}/DEL/user:token:${token}`);

            return true;
        } catch (error) {
            console.error('Logout error:', error);
            return false;
        }
    },

    /**
     * Get or create auth token for a screen name
     * This is the main entry point for user authentication
     * @param {string} screenName - The desired screen name
     * @returns {Promise<{success: boolean, token?: string, actorId?: string, error?: string}>}
     */
    async getOrCreateUser(screenName) {
        // Check if we have a token in session storage
        const existingToken = sessionStorage.getItem('authToken');

        if (existingToken) {
            // Verify existing token
            const verification = await this.verify(existingToken);
            if (verification.valid) {
                // Token is valid and matches our screen name
                if (verification.screenName === screenName) {
                    return {
                        success: true,
                        token: existingToken,
                        actorId: verification.actorId,
                        screenName: verification.screenName
                    };
                } else {
                    // Token exists but screen name doesn't match - logout old session
                    await this.logout(existingToken);
                    sessionStorage.removeItem('authToken');
                    sessionStorage.removeItem('screenName');
                    sessionStorage.removeItem('actorId');
                }
            } else {
                // Token is invalid - clean up
                sessionStorage.removeItem('authToken');
                sessionStorage.removeItem('screenName');
                sessionStorage.removeItem('actorId');
            }
        }

        // Register new user
        return await this.register(screenName);
    }
};

// Make Auth globally available
window.Auth = Auth;
