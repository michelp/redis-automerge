require('dotenv').config();
const express = require('express');
const session = require('express-session');
const passport = require('passport');
const GitHubStrategy = require('passport-github2').Strategy;
const redis = require('redis');
const RedisStore = require('connect-redis').default;
const crypto = require('crypto');

const app = express();
const PORT = process.env.PORT || 3000;

// Redis client for session storage and user data
const redisClient = redis.createClient({
  url: process.env.REDIS_URL || 'redis://redis:6379'
});

redisClient.connect().catch(console.error);

// Session configuration with Redis store
app.use(session({
  store: new RedisStore({ client: redisClient }),
  secret: process.env.SESSION_SECRET || 'change-me-in-production',
  resave: false,
  saveUninitialized: false,
  cookie: {
    secure: process.env.NODE_ENV === 'production',
    httpOnly: true,
    maxAge: 1000 * 60 * 60 * 24 * 7 // 7 days
  }
}));

// Initialize Passport
app.use(passport.initialize());
app.use(passport.session());

// Serialize user to session
passport.serializeUser((user, done) => {
  done(null, user.token);
});

// Deserialize user from session
passport.deserializeUser(async (token, done) => {
  try {
    const userData = await redisClient.hGetAll(`user:token:${token}`);
    if (!userData || Object.keys(userData).length === 0) {
      return done(null, false);
    }
    done(null, userData);
  } catch (err) {
    done(err);
  }
});

// GitHub OAuth Strategy
passport.use(new GitHubStrategy({
    clientID: process.env.GITHUB_CLIENT_ID,
    clientSecret: process.env.GITHUB_CLIENT_SECRET,
    callbackURL: process.env.GITHUB_CALLBACK_URL || 'http://localhost:8080/auth/github/callback'
  },
  async (accessToken, refreshToken, profile, done) => {
    try {
      // Generate unique token and actor ID for this user
      const token = crypto.randomBytes(32).toString('hex');
      const actorId = crypto.randomBytes(16).toString('hex');

      // Use GitHub username as screen name
      const screenName = profile.username;
      const provider = 'github';
      const providerId = profile.id;

      // Get avatar URL from profile
      const avatarUrl = profile.photos && profile.photos[0] ? profile.photos[0].value : null;

      const userData = {
        token,
        screenName,
        actorId,
        provider,
        providerId,
        avatarUrl: avatarUrl || '',
        created: Date.now().toString(),
        lastSeen: Date.now().toString()
      };

      // Store user data in Redis
      await redisClient.hSet(`user:token:${token}`, userData);
      await redisClient.sAdd(`user:name:${screenName}`, token);
      await redisClient.set(`user:actor:${actorId}`, token);
      await redisClient.set(`user:provider:${provider}:${providerId}`, token);

      // Set expiry on user data (7 days to match session)
      await redisClient.expire(`user:token:${token}`, 60 * 60 * 24 * 7);

      done(null, userData);
    } catch (err) {
      done(err);
    }
  }
));

// Routes

// Health check
app.get('/auth/health', (req, res) => {
  res.json({ status: 'ok' });
});

// Start GitHub OAuth flow
app.get('/auth/github', passport.authenticate('github', { scope: ['user:email'] }));

// GitHub OAuth callback
app.get('/auth/github/callback',
  passport.authenticate('github', { failureRedirect: '/' }),
  (req, res) => {
    // Successful authentication, redirect to editor
    res.redirect('/editor.html');
  }
);

// Get current session info
app.get('/auth/session', (req, res) => {
  if (req.isAuthenticated()) {
    res.json({
      authenticated: true,
      user: req.user
    });
  } else {
    res.json({
      authenticated: false
    });
  }
});

// Logout
app.post('/auth/logout', (req, res) => {
  req.logout((err) => {
    if (err) {
      return res.status(500).json({ error: 'Logout failed' });
    }
    req.session.destroy((err) => {
      if (err) {
        return res.status(500).json({ error: 'Session destruction failed' });
      }
      res.json({ success: true });
    });
  });
});

// Start server
app.listen(PORT, '0.0.0.0', () => {
  console.log(`Auth service listening on port ${PORT}`);
});
