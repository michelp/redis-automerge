#!/bin/bash
# Setup SSL certificates with Let's Encrypt for palimset.com

set -e

DOMAIN="palimset.com"
EMAIL="${1:-}"  # Email address passed as argument

if [ -z "${EMAIL}" ]; then
    echo "Error: Email address required"
    echo "Usage: $0 <email-address>"
    exit 1
fi

echo "=========================================="
echo "Setting up SSL with Let's Encrypt"
echo "=========================================="
echo "Domain: ${DOMAIN}"
echo ""

# Check if running on EC2 instance
if [ ! -f /opt/redis-automerge/.env ]; then
    echo "Error: This script must be run on the EC2 instance"
    echo "SSH into the instance first:"
    echo "  ./aws-deploy/manage.sh ssh"
    exit 1
fi

cd /opt/redis-automerge

# Install certbot if not already installed
if ! command -v certbot &> /dev/null; then
    echo "Installing certbot..."
    sudo dnf install -y certbot
    echo "✓ Certbot installed"
fi

# Create directory for Let's Encrypt validation
sudo mkdir -p /var/www/certbot

# Stop nginx temporarily to allow certbot to bind to port 80
echo "Stopping nginx temporarily..."
sudo docker-compose down demo

# Obtain certificate
echo "Obtaining SSL certificate..."
echo "This will send a certificate request to Let's Encrypt"
echo "Make sure ${DOMAIN} points to this server's IP address!"
echo ""

if [ ! -d "/etc/letsencrypt/live/${DOMAIN}" ]; then
    sudo certbot certonly \
        --standalone \
        --non-interactive \
        --agree-tos \
        --email "${EMAIL}" \
        --domains "${DOMAIN}" \
        --keep-until-expiring

    echo "✓ Certificate obtained"
else
    echo "✓ Certificate already exists"
fi

# Set correct permissions
sudo chmod 755 /etc/letsencrypt/live
sudo chmod 755 /etc/letsencrypt/archive

# Restart all services with production config
echo "Starting services with HTTPS configuration..."
sudo docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d

echo ""
echo "=========================================="
echo "SSL Setup Complete!"
echo "=========================================="
echo ""
echo "Your site is now available at:"
echo "  https://${DOMAIN}"
echo ""
echo "Certificate will auto-renew before expiration"
echo "Certbot timer will run daily to check for renewal"
echo ""
echo "To manually renew:"
echo "  sudo certbot renew"
echo ""
echo "=========================================="

# Setup automatic renewal
echo "Setting up automatic renewal..."
sudo systemctl enable certbot-renew.timer 2>/dev/null || \
    echo "Note: Manual renewal may be needed. Run: sudo certbot renew"

echo "✓ Setup complete"
