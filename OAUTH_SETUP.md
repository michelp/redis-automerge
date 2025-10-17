# OAuth Authentication Setup

This project now uses OAuth for authentication instead of simple screen names. All users must authenticate via OAuth (GitHub initially, with Google support coming soon).

## GitHub OAuth Setup

### 1. Create a GitHub OAuth App

1. Go to https://github.com/settings/developers
2. Click "New OAuth App"
3. Fill in the application details:
   - **Application name**: `Redis-Automerge` (or your preferred name)
   - **Homepage URL**:
     - For local development: `http://localhost:8080`
     - For production: `https://your-domain.com`
   - **Authorization callback URL**:
     - For local development: `http://localhost:8080/auth/github/callback`
     - For production: `https://your-domain.com/auth/github/callback`
4. Click "Register application"
5. Note your **Client ID** and **Client Secret** (keep the secret secure!)

### 2. Configure Environment Variables

1. Copy `.env.example` to `.env`:
   ```bash
   cp .env.example .env
   ```

2. Edit `.env` with your GitHub OAuth credentials:
   ```bash
   GITHUB_CLIENT_ID=your_github_client_id_here
   GITHUB_CLIENT_SECRET=your_github_client_secret_here
   GITHUB_CALLBACK_URL=http://localhost:8080/auth/github/callback
   SESSION_SECRET=$(node -e "console.log(require('crypto').randomBytes(32).toString('hex'))")
   NODE_ENV=development
   ```

3. Generate a secure SESSION_SECRET:
   ```bash
   node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"
   ```

### 3. Start the Application

```bash
# Start all services (including the new auth service)
docker compose up

# Or rebuild if you made changes
docker compose up --build
```

### 4. Access the Application

1. Open http://localhost:8080 in your browser
2. You'll see OAuth login buttons
3. Click "Sign in with GitHub"
4. Authorize the application
5. You'll be redirected back and automatically logged in

## OAuth Flow

1. **Landing Page** (`/index.html`):
   - Displays OAuth provider buttons
   - No guest mode - all users must authenticate

2. **GitHub OAuth Flow**:
   - User clicks "Sign in with GitHub"
   - Redirected to GitHub for authorization
   - GitHub redirects back to `/auth/github/callback`
   - Auth service creates session and user data in Redis
   - User redirected to editor with active session

3. **Session Management**:
   - Sessions stored in Redis (7-day expiry)
   - Session cookies are HTTP-only and secure (in production)
   - User data includes: token, screenName (GitHub username), actorId, provider, avatarUrl

4. **Editor Authentication** (`/editor.html`):
   - Checks for valid OAuth session
   - Redirects to login if not authenticated
   - Displays user avatar and provider info

## Redis Schema

OAuth authentication creates the following Redis keys:

```
user:token:{token}          → hash {screenName, actorId, provider, providerId, avatarUrl, created, lastSeen}
user:name:{screenName}      → set of tokens (for uniqueness)
user:actor:{actorId}        → token mapping
user:provider:{provider}:{providerId} → token mapping
```

Sessions are stored by connect-redis in:
```
sess:{sessionId} → serialized session data
```

## Security Notes

1. **Never commit `.env` file** - it's in `.gitignore`
2. **Keep Client Secret secure** - never expose it in client-side code
3. **Use HTTPS in production** - OAuth requires secure callback URLs
4. **Rotate SESSION_SECRET** periodically in production
5. **Set NODE_ENV=production** in production to enable secure cookies

## Adding Google OAuth (Future)

The architecture is ready for multiple OAuth providers. To add Google:

1. Create a Google OAuth App at https://console.developers.google.com
2. Add `passport-google-oauth20` to `auth-service/package.json`
3. Configure Google strategy in `auth-service/server.js`
4. Add Google button to `demo/index.html`
5. Update `.env.example` with Google credentials

## Troubleshooting

### "Failed to authenticate" error
- Check that GitHub OAuth App callback URL matches your GITHUB_CALLBACK_URL
- Verify CLIENT_ID and CLIENT_SECRET are correct
- Check auth service logs: `docker compose logs auth`

### Session not persisting
- Verify Redis is running: `docker compose ps redis`
- Check SESSION_SECRET is set in `.env`
- For production, ensure cookies are secure and HTTPS is enabled

### Avatar not displaying
- GitHub user may not have a public avatar
- Check browser console for image load errors
- Verify avatarUrl is stored in sessionStorage

## Development vs Production

**Local Development:**
- Use `http://localhost:8080` for all URLs
- SESSION_SECRET can be any random string
- NODE_ENV=development allows insecure cookies

**Production:**
- Use `https://your-domain.com` for all URLs
- Generate strong SESSION_SECRET: `openssl rand -hex 32`
- Set NODE_ENV=production for secure cookies
- Configure your GitHub OAuth App with production callback URL

## Migration from Old Authentication

The old screen-name-based authentication has been completely removed. Users must now:
1. Sign in with GitHub (or future OAuth providers)
2. Their GitHub username becomes their screen name
3. All existing screen names will be available for new OAuth users to claim
