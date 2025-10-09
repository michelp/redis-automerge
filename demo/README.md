# Redis-Automerge Demo Frontend

A web-based demo application for interacting with Redis-Automerge through the Webdis HTTP interface.

## Features

- **Interactive UI** for all Redis-Automerge commands
- **Real-time connection status** monitoring
- **Command logging** with color-coded output
- **Pre-built examples** demonstrating common use cases
- **Support for all data types**: text, integers, doubles, booleans
- **List operations** with append and length queries
- **Nested path syntax** support (e.g., `user.profile.name`, `items[0]`)

## Quick Start

### Using Docker Compose

```bash
# Start all services (Redis, Webdis, and Demo frontend)
docker compose up

# Access the demo at:
# http://localhost:8080
```

The demo will automatically connect to Webdis at `http://localhost:7379`.

### Manual Setup

If running outside Docker Compose:

1. Update `WEBDIS_URL` in `app.js` to point to your Webdis instance
2. Serve the `demo` directory with any web server:
   ```bash
   # Using Python
   python -m http.server 8080

   # Using Node.js
   npx serve

   # Using nginx (already configured in docker-compose)
   ```

## Usage

### Document Management

1. Enter a document key (e.g., `mydoc`)
2. Click **Create Document** to initialize a new Automerge document
3. Use the various operations to manipulate the document

### Operations

**Text Operations**
- Set/get string values at any path
- Example: `user.name` → `"Alice"`

**Integer Operations**
- Set/get integer values
- Example: `user.age` → `28`

**Double Operations**
- Set/get floating-point values
- Example: `metrics.cpu` → `75.5`

**Boolean Operations**
- Set/get true/false values
- Example: `user.active` → `true`

**List Operations**
- Create lists at any path
- Append values (text, int, double, bool)
- Query list length
- Access items by index: `items[0]`

### Path Syntax

The demo supports RedisJSON-compatible paths:

```
# Simple keys
name
age

# Nested objects (dot notation)
user.profile.name
config.database.host

# Array indices
items[0]
users[5].email

# JSONPath style ($ prefix)
$.user.name
$.items[0].title
```

### Quick Examples

Click any of the example buttons to run pre-configured scenarios:

- **User Profile** - Create a user with nested profile data
- **Shopping Cart** - Build a cart with items list
- **Configuration** - Set up a config document with feature flags

## API Endpoint

The demo communicates with Webdis using HTTP GET requests:

```
GET http://localhost:7379/AM.NEW/mydoc
GET http://localhost:7379/AM.PUTTEXT/mydoc/user.name/Alice
GET http://localhost:7379/AM.GETTEXT/mydoc/user.name
```

## Architecture

```
┌─────────────┐      HTTP      ┌─────────┐    Redis     ┌───────┐
│   Browser   │ ──────────────> │ Webdis  │ ──────────> │ Redis │
│  (Demo UI)  │ <────────────── │  (7379) │ <────────── │ (6379)│
└─────────────┘      JSON       └─────────┘   Protocol   └───────┘
                                                            │
                                                            │
                                                    ┌───────────────┐
                                                    │ redis-        │
                                                    │ automerge.so  │
                                                    └───────────────┘
```

## Files

- `index.html` - Main UI structure
- `style.css` - Styling and responsive design
- `app.js` - Application logic and Webdis communication
- `nginx.conf` - Nginx configuration with CORS support and API proxy

## Development

To modify the demo:

1. Edit the HTML/CSS/JS files in the `demo/` directory
2. Refresh your browser to see changes (no build step required)
3. Check the browser console for debugging information

## CORS Configuration

The demo handles CORS in two ways:

1. **Webdis** is configured with `http_access_control_allow_origin: "*"` in `webdis.json`
2. **Nginx** includes CORS headers and can proxy requests to `/api/` → Webdis

## Troubleshooting

**"Disconnected" status**
- Ensure all Docker services are running: `docker compose ps`
- Check Webdis logs: `docker compose logs webdis`
- Verify Webdis is accessible: `curl http://localhost:7379/PING`

**Commands not working**
- Check Redis logs: `docker compose logs redis`
- Verify the module is loaded: `docker compose exec redis redis-cli MODULE LIST`
- Check browser console for JavaScript errors

**Network errors**
- Ensure ports 8080 (demo) and 7379 (Webdis) are not in use
- Check Docker network: `docker compose network ls`
