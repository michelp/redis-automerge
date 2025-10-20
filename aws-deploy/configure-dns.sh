#!/bin/bash
# Configure Route53 DNS for redis-automerge deployment

set -e

# Load configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/aws-config.sh"

# Load instance info
if [ ! -f "${SCRIPT_DIR}/instance-info.txt" ]; then
    echo "Error: instance-info.txt not found"
    echo "Please run deploy.sh first"
    exit 1
fi

source "${SCRIPT_DIR}/instance-info.txt"

echo "=========================================="
echo "Route53 DNS Configuration"
echo "=========================================="
echo ""

# Check if domain is configured
if [ -z "${DOMAIN}" ]; then
    echo "Error: DOMAIN is not set in aws-config.sh"
    echo "Please edit aws-config.sh and set DOMAIN to your domain name"
    echo "Example: export DOMAIN=\"palimset.example.com\""
    exit 1
fi

echo "Domain: ${DOMAIN}"
echo "Elastic IP: ${ELASTIC_IP}"
echo ""

# Check AWS CLI is installed
if ! command -v aws &> /dev/null; then
    echo "Error: AWS CLI is not installed"
    exit 1
fi

# Check AWS credentials
if ! aws sts get-caller-identity &> /dev/null; then
    echo "Error: AWS credentials not configured"
    exit 1
fi

# Extract root domain (e.g., "example.com" from "palimset.example.com")
# This handles both subdomain.example.com and example.com formats
ROOT_DOMAIN=$(echo "${DOMAIN}" | awk -F. '{print $(NF-1)"."$NF}')

echo "Looking for hosted zone for root domain: ${ROOT_DOMAIN}"

# Get or create hosted zone
HOSTED_ZONE_ID=$(aws route53 list-hosted-zones \
    --query "HostedZones[?Name=='${ROOT_DOMAIN}.'].Id" \
    --output text 2>/dev/null | head -n1)

if [ -z "${HOSTED_ZONE_ID}" ] || [ "${HOSTED_ZONE_ID}" == "None" ]; then
    echo "Hosted zone not found. Creating new hosted zone for ${ROOT_DOMAIN}..."

    # Create hosted zone
    CALLER_REFERENCE="redis-automerge-$(date +%s)"
    CREATE_OUTPUT=$(aws route53 create-hosted-zone \
        --name "${ROOT_DOMAIN}" \
        --caller-reference "${CALLER_REFERENCE}" \
        --hosted-zone-config "Comment=Created for redis-automerge deployment" \
        --output json)

    HOSTED_ZONE_ID=$(echo "${CREATE_OUTPUT}" | grep -o '"/hostedzone/[^"]*"' | cut -d'"' -f2)

    echo "✓ Hosted zone created: ${HOSTED_ZONE_ID}"
    echo ""

    # Get nameservers
    NAMESERVERS=$(aws route53 get-hosted-zone \
        --id "${HOSTED_ZONE_ID}" \
        --query 'DelegationSet.NameServers' \
        --output text)

    echo "=========================================="
    echo "IMPORTANT: Configure your domain registrar"
    echo "=========================================="
    echo ""
    echo "You need to update your domain's nameservers at your domain registrar"
    echo "(e.g., GoDaddy, Namecheap, Google Domains, etc.)"
    echo ""
    echo "Set the nameservers to:"
    echo "${NAMESERVERS}" | tr '\t' '\n' | while read ns; do
        echo "  - ${ns}"
    done
    echo ""
    echo "DNS propagation may take 24-48 hours"
    echo "=========================================="
    echo ""
else
    echo "✓ Found existing hosted zone: ${HOSTED_ZONE_ID}"
    echo ""
fi

# Create A record for the domain
echo "Creating/updating DNS A record..."

# Prepare the change batch JSON
CHANGE_BATCH=$(cat <<EOF
{
  "Changes": [
    {
      "Action": "UPSERT",
      "ResourceRecordSet": {
        "Name": "${DOMAIN}",
        "Type": "A",
        "TTL": 300,
        "ResourceRecords": [
          {
            "Value": "${ELASTIC_IP}"
          }
        ]
      }
    }
  ]
}
EOF
)

# Create the change
CHANGE_ID=$(aws route53 change-resource-record-sets \
    --hosted-zone-id "${HOSTED_ZONE_ID}" \
    --change-batch "${CHANGE_BATCH}" \
    --query 'ChangeInfo.Id' \
    --output text)

echo "✓ DNS record created/updated: ${DOMAIN} -> ${ELASTIC_IP}"
echo "  Change ID: ${CHANGE_ID}"
echo ""

# Wait for change to propagate
echo "Waiting for DNS change to propagate..."
aws route53 wait resource-record-sets-changed --id "${CHANGE_ID}"
echo "✓ DNS change propagated"
echo ""

echo "=========================================="
echo "DNS Configuration Complete!"
echo "=========================================="
echo ""
echo "Domain: ${DOMAIN}"
echo "Points to: ${ELASTIC_IP}"
echo ""
echo "Your application will be available at:"
echo "  http://${DOMAIN}:${HTTP_PORT}"
echo ""
echo "Next steps:"
echo "  1. Update your .env file with the domain-based callback URL"
echo "  2. Restart the application to use the new domain"
echo ""
echo "Note: If you created a new hosted zone, remember to update"
echo "your domain registrar's nameservers (see output above)"
echo ""
echo "=========================================="

# Save DNS info to instance-info.txt
cat >> "${SCRIPT_DIR}/instance-info.txt" << EOF
DOMAIN=${DOMAIN}
HOSTED_ZONE_ID=${HOSTED_ZONE_ID}
DNS_CONFIGURED_AT="$(date)"
EOF

echo "DNS info appended to: ${SCRIPT_DIR}/instance-info.txt"
