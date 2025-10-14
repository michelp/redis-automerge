#!/bin/bash
# Teardown script - removes all AWS resources

set -e

# Load configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/aws-config.sh"

echo "=========================================="
echo "Redis-Automerge AWS Teardown"
echo "=========================================="
echo ""
echo "This will DELETE all resources:"
echo "  - EC2 instance"
echo "  - Security group"
echo "  - SSH key pair (from AWS, local file will remain)"
echo ""

# Load instance info if exists
INSTANCE_ID=""
if [ -f "${SCRIPT_DIR}/instance-info.txt" ]; then
    source "${SCRIPT_DIR}/instance-info.txt"
fi

# Confirm deletion
read -p "Are you sure you want to proceed? (yes/no): " CONFIRM
if [ "${CONFIRM}" != "yes" ]; then
    echo "Teardown cancelled"
    exit 0
fi

echo ""

# Step 1: Terminate EC2 instance
if [ -n "${INSTANCE_ID}" ]; then
    echo "Terminating instance ${INSTANCE_ID}..."

    # Check if instance exists
    STATE=$(aws ec2 describe-instances \
        --instance-ids "${INSTANCE_ID}" \
        --region "${REGION}" \
        --query 'Reservations[0].Instances[0].State.Name' \
        --output text 2>/dev/null || echo "")

    if [ -n "${STATE}" ] && [ "${STATE}" != "terminated" ]; then
        aws ec2 terminate-instances \
            --instance-ids "${INSTANCE_ID}" \
            --region "${REGION}"

        echo "Waiting for instance to terminate..."
        aws ec2 wait instance-terminated \
            --instance-ids "${INSTANCE_ID}" \
            --region "${REGION}"

        echo "✓ Instance terminated: ${INSTANCE_ID}"
    else
        echo "✓ Instance already terminated or not found"
    fi
else
    # Try to find instance by name
    echo "Looking for instance by name tag..."
    INSTANCE_ID=$(aws ec2 describe-instances \
        --filters "Name=tag:Name,Values=${INSTANCE_NAME}" "Name=instance-state-name,Values=running,stopped" \
        --region "${AWS_REGION}" \
        --query 'Reservations[0].Instances[0].InstanceId' \
        --output text 2>/dev/null || echo "")

    if [ -n "${INSTANCE_ID}" ] && [ "${INSTANCE_ID}" != "None" ]; then
        echo "Found instance: ${INSTANCE_ID}"
        echo "Terminating..."

        aws ec2 terminate-instances \
            --instance-ids "${INSTANCE_ID}" \
            --region "${AWS_REGION}"

        echo "Waiting for instance to terminate..."
        aws ec2 wait instance-terminated \
            --instance-ids "${INSTANCE_ID}" \
            --region "${AWS_REGION}"

        echo "✓ Instance terminated: ${INSTANCE_ID}"
    else
        echo "✓ No running instance found"
    fi
fi
echo ""

# Step 2: Delete security group
echo "Deleting security group..."
SECURITY_GROUP_ID=$(aws ec2 describe-security-groups \
    --group-names "${SECURITY_GROUP_NAME}" \
    --region "${AWS_REGION}" \
    --query 'SecurityGroups[0].GroupId' \
    --output text 2>/dev/null || echo "")

if [ -n "${SECURITY_GROUP_ID}" ] && [ "${SECURITY_GROUP_ID}" != "None" ]; then
    # Wait a bit for instance to fully terminate
    sleep 10

    aws ec2 delete-security-group \
        --group-id "${SECURITY_GROUP_ID}" \
        --region "${AWS_REGION}"

    echo "✓ Security group deleted: ${SECURITY_GROUP_ID}"
else
    echo "✓ Security group not found or already deleted"
fi
echo ""

# Step 3: Delete SSH key pair (from AWS only)
echo "Deleting SSH key pair from AWS..."
KEY_EXISTS=$(aws ec2 describe-key-pairs \
    --key-names "${KEY_NAME}" \
    --region "${AWS_REGION}" \
    --query 'KeyPairs[0].KeyName' \
    --output text 2>/dev/null || echo "")

if [ -n "${KEY_EXISTS}" ] && [ "${KEY_EXISTS}" != "None" ]; then
    aws ec2 delete-key-pair \
        --key-name "${KEY_NAME}" \
        --region "${AWS_REGION}"

    echo "✓ SSH key pair deleted from AWS: ${KEY_NAME}"
    echo "  (Local key file preserved: ${KEY_FILE})"
else
    echo "✓ SSH key pair not found in AWS"
fi
echo ""

# Step 4: Clean up local files
echo "Cleaning up local deployment files..."
if [ -f "${SCRIPT_DIR}/instance-info.txt" ]; then
    mv "${SCRIPT_DIR}/instance-info.txt" "${SCRIPT_DIR}/instance-info.txt.bak"
    echo "✓ instance-info.txt backed up"
fi
echo ""

# Offer to delete SSH key file
if [ -f "${KEY_FILE}" ]; then
    echo "Local SSH key file still exists: ${KEY_FILE}"
    read -p "Delete local SSH key file? (yes/no): " DELETE_KEY
    if [ "${DELETE_KEY}" == "yes" ]; then
        rm "${KEY_FILE}"
        echo "✓ Local SSH key file deleted"
    else
        echo "✓ Local SSH key file preserved"
    fi
fi
echo ""

echo "=========================================="
echo "Teardown Complete!"
echo "=========================================="
echo ""
echo "All AWS resources have been removed."
echo "You will no longer be charged for these resources."
echo ""
echo "To redeploy, run: ./aws-deploy/deploy.sh"
echo ""
