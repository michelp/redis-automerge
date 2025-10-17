// ============================================================================
// Auth Module - OAuth session management
// ============================================================================

const Auth = {
    WEBDIS_URL: `${window.location.protocol}//${window.location.host}/api`,
    AUTH_URL: `${window.location.protocol}//${window.location.host}/auth`,

    /**
     * Check current OAuth session status
     * @returns {Promise<{authenticated: boolean, user?: object}>}
     */
    async checkSession() {
        try {
            const response = await fetch(`${this.AUTH_URL}/session`, {
                credentials: 'include'
            });
            const data = await response.json();
            return data;
        } catch (error) {
            console.error('Session check error:', error);
            return {
                authenticated: false,
                error: error.message
            };
        }
    },

    /**
     * Verify a token and get user data from Redis
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
                actorId: userData.actorId,
                provider: userData.provider,
                avatarUrl: userData.avatarUrl
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
     * Logout - destroy OAuth session and clean up
     * @returns {Promise<boolean>} True if successful
     */
    async logout() {
        try {
            const response = await fetch(`${this.AUTH_URL}/logout`, {
                method: 'POST',
                credentials: 'include'
            });

            if (!response.ok) {
                throw new Error('Logout request failed');
            }

            // Clean up session storage
            sessionStorage.removeItem('authToken');
            sessionStorage.removeItem('screenName');
            sessionStorage.removeItem('actorId');
            sessionStorage.removeItem('provider');
            sessionStorage.removeItem('avatarUrl');

            return true;
        } catch (error) {
            console.error('Logout error:', error);
            return false;
        }
    },

    /**
     * Ensure user is authenticated, redirect to login if not
     * @returns {Promise<{token: string, screenName: string, actorId: string} | null>}
     */
    async requireAuth() {
        // Check sessionStorage first
        const token = sessionStorage.getItem('authToken');
        const screenName = sessionStorage.getItem('screenName');
        const actorId = sessionStorage.getItem('actorId');

        if (token && screenName && actorId) {
            // Verify token is still valid
            const verification = await this.verify(token);
            if (verification.valid) {
                return { token, screenName, actorId };
            }
        }

        // Check if we have a server session
        const session = await this.checkSession();
        if (session.authenticated && session.user) {
            // Store in sessionStorage
            sessionStorage.setItem('authToken', session.user.token);
            sessionStorage.setItem('screenName', session.user.screenName);
            sessionStorage.setItem('actorId', session.user.actorId);
            sessionStorage.setItem('provider', session.user.provider);
            if (session.user.avatarUrl) {
                sessionStorage.setItem('avatarUrl', session.user.avatarUrl);
            }

            return {
                token: session.user.token,
                screenName: session.user.screenName,
                actorId: session.user.actorId
            };
        }

        // Not authenticated - redirect to login
        window.location.href = '/index.html';
        return null;
    }
};

// Make Auth globally available
window.Auth = Auth;
