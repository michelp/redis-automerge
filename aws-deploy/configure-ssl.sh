#!/bin/bash
# Configure SSL/HTTPS for production deployment

set -e

# Load configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/aws-config.sh"

# Load instance info
if [ ! -f "${SCRIPT_DIR}/instance-info.txt" ]; then
    echo "Error: No instance info found. Run deploy.sh first."
    exit 1
fi

source "${SCRIPT_DIR}/instance-info.txt"

echo "=========================================="
echo "SSL/HTTPS Configuration"
echo "=========================================="
echo "Domain: ${DOMAIN}"
echo "Instance: ${INSTANCE_ID}"
echo "IP: ${ELASTIC_IP}"
echo ""

if [ -z "${DOMAIN}" ]; then
    echo "Error: DOMAIN is not set in aws-config.sh"
    echo "Please set DOMAIN before configuring SSL"
    exit 1
fi

echo "This will:"
echo "1. Install certbot on the server"
echo "2. Obtain Let's Encrypt SSL certificate"
echo "3. Configure nginx for HTTPS"
echo "4. Restart services with HTTPS enabled"
echo ""

# Prompt for email if not provided
if [ -z "${1:-}" ]; then
    read -p "Enter your email address for Let's Encrypt notifications: " EMAIL
else
    EMAIL="${1}"
fi

echo ""
echo "Email: ${EMAIL}"
echo ""
read -p "Continue? (y/n) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Cancelled"
    exit 1
fi

# Copy SSL setup script to server
echo "Uploading SSL setup script..."
scp -o StrictHostKeyChecking=no -i "${KEY_FILE}" \
    "${SCRIPT_DIR}/setup-ssl.sh" \
    "ec2-user@${ELASTIC_IP}:/tmp/setup-ssl.sh"

# Run SSL setup on server
echo "Running SSL setup on server..."
echo "You may be prompted to agree to Let's Encrypt Terms of Service"
echo ""

ssh -o StrictHostKeyChecking=no -i "${KEY_FILE}" "ec2-user@${ELASTIC_IP}" << EOF
    cd /opt/redis-automerge
    chmod +x /tmp/setup-ssl.sh
    sudo /tmp/setup-ssl.sh "${EMAIL}"
    rm /tmp/setup-ssl.sh
EOF

echo ""
echo "=========================================="
echo "SSL Configuration Complete!"
echo "=========================================="
echo ""
echo "Your site is now available at:"
echo "  https://${DOMAIN}"
echo ""
echo "HTTP traffic is automatically redirected to HTTPS"
echo "Certificate will auto-renew before expiration"
echo ""
echo "Next steps:"
echo "1. Create GitHub OAuth app with callback: https://${DOMAIN}/auth/github/callback"
echo "2. Update .env.production with new OAuth credentials"
echo "3. Run: ./aws-deploy/manage.sh update"
echo ""
echo "=========================================="
