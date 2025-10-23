# Docker Hub Publishing Setup

This document explains how to set up automatic Docker image publishing to Docker Hub using GitHub Actions.

## Overview

The repository is configured to automatically build, test, and publish Docker images to Docker Hub:
- **Image name**: `metagration/redis-automerge`
- **Registry**: Docker Hub (hub.docker.com)
- **Triggers**: Pushes to main/master branch and version tags
- **Testing**: All integration tests must pass before pushing

## Tagging Strategy

### Version Tags (Recommended for Releases)

When you create a Git tag following semantic versioning (e.g., `v1.2.3`), the workflow generates multiple Docker tags:

```bash
# Example: Creating tag v1.2.3 generates:
metagration/redis-automerge:1.2.3    # Full version
metagration/redis-automerge:1.2      # Minor version
metagration/redis-automerge:1        # Major version
metagration/redis-automerge:latest   # Latest stable
```

### Branch Tags

Pushes to main/master branch:
```bash
metagration/redis-automerge:latest   # Latest development
metagration/redis-automerge:main-abc123f  # Branch + commit SHA
```

### Pull Request Tags

Pull requests are built and tested but NOT pushed to Docker Hub:
```bash
metagration/redis-automerge:pr-42    # PR number (local only)
```

## Initial Setup (One-Time)

### Step 1: Create Docker Hub Access Token

1. Go to [Docker Hub Account Settings](https://hub.docker.com/settings/security)
2. Click **"New Access Token"**
3. Give it a descriptive name: `redis-automerge-github-actions`
4. Select permissions: **Read, Write, Delete**
5. Click **"Generate"**
6. **Copy the token immediately** (you won't be able to see it again)

### Step 2: Add Secrets to GitHub Repository

1. Go to your GitHub repository: `https://github.com/YOUR_USERNAME/redis-automerge`
2. Click **Settings** → **Secrets and variables** → **Actions**
3. Click **"New repository secret"**
4. Add two secrets:

**Secret 1: DOCKERHUB_USERNAME**
- Name: `DOCKERHUB_USERNAME`
- Value: `metagration`

**Secret 2: DOCKERHUB_TOKEN**
- Name: `DOCKERHUB_TOKEN`
- Value: *[paste the token from Step 1]*

### Step 3: Verify Workflow File

The workflow file is already created at `.github/workflows/docker-publish.yml`. Verify it's committed to your repository:

```bash
git add .github/workflows/docker-publish.yml
git commit -m "Add Docker Hub publishing workflow"
git push origin main
```

## Usage

### Publishing Development Builds

Simply push to the main branch:

```bash
git add .
git commit -m "Update feature"
git push origin main
```

This will:
1. Build the Docker image
2. Run all integration tests
3. If tests pass, push to Docker Hub as `latest`

### Publishing Release Versions

Create and push a version tag:

```bash
# Create a new version tag
git tag -a v1.0.0 -m "Release version 1.0.0"

# Push the tag to GitHub
git push origin v1.0.0
```

This will:
1. Build the Docker image
2. Run all integration tests
3. If tests pass, push to Docker Hub with tags:
   - `metagration/redis-automerge:1.0.0`
   - `metagration/redis-automerge:1.0`
   - `metagration/redis-automerge:1`
   - `metagration/redis-automerge:latest`

### Viewing Build Status

1. Go to your GitHub repository
2. Click the **Actions** tab
3. View the "Build and Publish Docker Image" workflow runs
4. Click on any run to see detailed logs

## Using Published Images

### Pull Latest Version

```bash
docker pull metagration/redis-automerge:latest
```

### Pull Specific Version

```bash
docker pull metagration/redis-automerge:1.0.0
```

### Run the Image

```bash
# Run Redis with the module loaded
docker run -d -p 6379:6379 metagration/redis-automerge:latest

# Test it works
redis-cli PING
redis-cli AM.NEW testdoc
redis-cli AM.PUTTEXT testdoc name "Hello World"
redis-cli AM.GETTEXT testdoc name
```

### Use in Docker Compose

Update your `docker-compose.yml`:

```yaml
services:
  redis:
    image: metagration/redis-automerge:latest
    # or pin to specific version:
    # image: metagration/redis-automerge:1.0.0
    ports:
      - "6379:6379"
```

## Semantic Versioning Guidelines

Follow [Semantic Versioning 2.0.0](https://semver.org/):

- **MAJOR** version (v1.0.0 → v2.0.0): Incompatible API changes
- **MINOR** version (v1.0.0 → v1.1.0): New functionality, backward compatible
- **PATCH** version (v1.0.0 → v1.0.1): Bug fixes, backward compatible

### Examples

```bash
# Initial release
git tag -a v1.0.0 -m "Initial release"

# Bug fix
git tag -a v1.0.1 -m "Fix timestamp persistence bug"

# New feature (marks support)
git tag -a v1.1.0 -m "Add rich text marks support"

# Breaking change
git tag -a v2.0.0 -m "Change JSON output format for counters"

# Push all tags
git push origin --tags
```

## Troubleshooting

### Build Fails

Check the GitHub Actions logs:
1. Go to **Actions** tab
2. Click the failed workflow run
3. Expand the failed step to see error details

### Authentication Fails

Error: `denied: requested access to the resource is denied`

**Solutions:**
1. Verify `DOCKERHUB_USERNAME` secret is set to `metagration`
2. Verify `DOCKERHUB_TOKEN` secret contains a valid access token
3. Check token hasn't expired (regenerate if needed)
4. Verify token has **Write** permissions

### Tests Fail

The workflow will NOT push images if tests fail. Fix the tests first:

```bash
# Run tests locally
docker compose run --build --rm test
docker compose down
```

### Wrong Tags Generated

Verify your Git tag follows the format `vX.Y.Z` (note the `v` prefix):
- ✅ Correct: `v1.0.0`, `v2.1.3`
- ❌ Wrong: `1.0.0`, `release-1.0`, `v1.0`

## Security Best Practices

1. **Never commit Docker Hub credentials** to the repository
2. **Use access tokens** instead of your Docker Hub password
3. **Rotate tokens regularly** (every 6-12 months)
4. **Use least privilege**: Only grant necessary permissions
5. **Monitor access logs** in Docker Hub settings

## Advanced: Manual Docker Push

If you need to push manually (bypassing GitHub Actions):

```bash
# Build the image locally
docker compose build redis

# Tag with your desired version
docker tag redis-automerge-redis metagration/redis-automerge:1.0.0
docker tag redis-automerge-redis metagration/redis-automerge:latest

# Login to Docker Hub
docker login -u metagration

# Push the tags
docker push metagration/redis-automerge:1.0.0
docker push metagration/redis-automerge:latest
```

## Monitoring

### Docker Hub Stats

View your image stats at:
- https://hub.docker.com/r/metagration/redis-automerge

You can see:
- Pull count
- Recent pushes
- Available tags
- Image size

### GitHub Actions Logs

All builds are logged in GitHub Actions. You can:
- View build duration
- See test results
- Download build artifacts
- Get notified of failures via email

## Support

For issues with:
- **Workflow/CI/CD**: Check GitHub Actions documentation
- **Docker Hub**: Check Docker Hub documentation
- **Module functionality**: See the main README.md
