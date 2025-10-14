#!/bin/bash
# EC2 User Data Script
# This runs automatically on first boot

set -e

# Log everything
exec > >(tee /var/log/user-data.log)
exec 2>&1

echo "=========================================="
echo "Starting redis-automerge setup"
echo "Time: $(date)"
echo "=========================================="

# Update system
echo "Updating system packages..."
dnf update -y

# Install Docker
echo "Installing Docker..."
dnf install -y docker

# Start and enable Docker
echo "Starting Docker service..."
systemctl start docker
systemctl enable docker

# Install Docker Compose
echo "Installing Docker Compose..."
DOCKER_COMPOSE_VERSION="v2.24.5"
curl -L "https://github.com/docker/compose/releases/download/${DOCKER_COMPOSE_VERSION}/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose
chmod +x /usr/local/bin/docker-compose
ln -sf /usr/local/bin/docker-compose /usr/bin/docker-compose

# Install git
echo "Installing git..."
dnf install -y git

# Create app directory
echo "Creating application directory..."
mkdir -p /opt/redis-automerge
cd /opt/redis-automerge

# Create a flag file to indicate setup is complete
echo "Setup complete at $(date)" > /opt/redis-automerge/setup-complete.txt

echo "=========================================="
echo "Setup script completed successfully"
echo "Time: $(date)"
echo "=========================================="

# Note: The actual application deployment will be done via SCP or git clone
# after the instance is running. This is because we need to wait for
# the instance to be fully accessible before copying files.
