#!/bin/bash
# Main deployment script for redis-automerge on AWS EC2

set -e

# Load configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/aws-config.sh"

echo "=========================================="
echo "Redis-Automerge AWS Deployment"
echo "=========================================="
echo "Region: ${AWS_REGION}"
echo "Instance Type: ${INSTANCE_TYPE}"
echo "Instance Name: ${INSTANCE_NAME}"
echo ""

# Check AWS CLI is installed
if ! command -v aws &> /dev/null; then
    echo "Error: AWS CLI is not installed"
    echo "Install: https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html"
    exit 1
fi

# Check AWS credentials are configured
if ! aws sts get-caller-identity &> /dev/null; then
    echo "Error: AWS credentials not configured"
    echo "Run: aws configure"
    exit 1
fi

echo "✓ AWS CLI configured"
echo ""

# Step 1: Create SSH key pair if it doesn't exist
if [ ! -f "${KEY_FILE}" ]; then
    echo "Creating SSH key pair..."
    aws ec2 create-key-pair \
        --key-name "${KEY_NAME}" \
        --query 'KeyMaterial' \
        --output text \
        --region "${AWS_REGION}" > "${KEY_FILE}"
    chmod 400 "${KEY_FILE}"
    echo "✓ SSH key created: ${KEY_FILE}"
else
    echo "✓ SSH key already exists: ${KEY_FILE}"
fi
echo ""

# Step 2: Get VPC ID and create security group
echo "Getting VPC information..."

# Get VPC ID (try default first, then use any available VPC)
VPC_ID=$(aws ec2 describe-vpcs \
    --region "${AWS_REGION}" \
    --filters "Name=is-default,Values=true" \
    --query 'Vpcs[0].VpcId' \
    --output text 2>/dev/null || echo "")

if [ -z "${VPC_ID}" ] || [ "${VPC_ID}" == "None" ]; then
    VPC_ID=$(aws ec2 describe-vpcs \
        --region "${AWS_REGION}" \
        --query 'Vpcs[0].VpcId' \
        --output text)
fi

echo "Using VPC: ${VPC_ID}"

echo "Creating security group..."
SECURITY_GROUP_ID=$(aws ec2 describe-security-groups \
    --group-names "${SECURITY_GROUP_NAME}" \
    --region "${AWS_REGION}" \
    --query 'SecurityGroups[0].GroupId' \
    --output text 2>/dev/null || echo "")

if [ -z "${SECURITY_GROUP_ID}" ] || [ "${SECURITY_GROUP_ID}" == "None" ]; then
    SECURITY_GROUP_ID=$(aws ec2 create-security-group \
        --group-name "${SECURITY_GROUP_NAME}" \
        --description "Security group for redis-automerge demo" \
        --vpc-id "${VPC_ID}" \
        --region "${AWS_REGION}" \
        --query 'GroupId' \
        --output text)

    echo "✓ Security group created: ${SECURITY_GROUP_ID}"

    # Add SSH rule
    aws ec2 authorize-security-group-ingress \
        --group-id "${SECURITY_GROUP_ID}" \
        --protocol tcp \
        --port 22 \
        --cidr 0.0.0.0/0 \
        --region "${AWS_REGION}"
    echo "  - Opened port 22 (SSH)"

    # Add HTTP rule
    aws ec2 authorize-security-group-ingress \
        --group-id "${SECURITY_GROUP_ID}" \
        --protocol tcp \
        --port "${HTTP_PORT}" \
        --cidr 0.0.0.0/0 \
        --region "${AWS_REGION}"
    echo "  - Opened port ${HTTP_PORT} (HTTP)"

    # Add Redis rule (optional - for external access)
    # Uncomment if you need external Redis access
    # aws ec2 authorize-security-group-ingress \
    #     --group-id "${SECURITY_GROUP_ID}" \
    #     --protocol tcp \
    #     --port "${REDIS_PORT}" \
    #     --cidr 0.0.0.0/0 \
    #     --region "${AWS_REGION}"

    # Add Webdis rule (required for frontend access)
    aws ec2 authorize-security-group-ingress \
        --group-id "${SECURITY_GROUP_ID}" \
        --protocol tcp \
        --port "${WEBDIS_PORT}" \
        --cidr 0.0.0.0/0 \
        --region "${AWS_REGION}"
    echo "  - Opened port ${WEBDIS_PORT} (Webdis)"
else
    echo "✓ Security group already exists: ${SECURITY_GROUP_ID}"
fi
echo ""

# Step 3: Launch EC2 instance
echo "Launching EC2 instance..."

# Get a subnet from the VPC (preferably with MapPublicIpOnLaunch=true)
SUBNET_ID=$(aws ec2 describe-subnets \
    --region "${AWS_REGION}" \
    --filters "Name=vpc-id,Values=${VPC_ID}" "Name=map-public-ip-on-launch,Values=true" \
    --query 'Subnets[0].SubnetId' \
    --output text 2>/dev/null || echo "")

if [ -z "${SUBNET_ID}" ] || [ "${SUBNET_ID}" == "None" ]; then
    # If no subnet with auto-assign public IP, get any subnet
    SUBNET_ID=$(aws ec2 describe-subnets \
        --region "${AWS_REGION}" \
        --filters "Name=vpc-id,Values=${VPC_ID}" \
        --query 'Subnets[0].SubnetId' \
        --output text)
fi

echo "Using subnet: ${SUBNET_ID}"

INSTANCE_ID=$(aws ec2 run-instances \
    --image-id "${AMI_ID}" \
    --instance-type "${INSTANCE_TYPE}" \
    --key-name "${KEY_NAME}" \
    --security-group-ids "${SECURITY_GROUP_ID}" \
    --subnet-id "${SUBNET_ID}" \
    --associate-public-ip-address \
    --user-data "file://${SCRIPT_DIR}/setup.sh" \
    --tag-specifications "${TAGS}" \
    --region "${AWS_REGION}" \
    --query 'Instances[0].InstanceId' \
    --output text)

echo "✓ Instance launched: ${INSTANCE_ID}"
echo "  Waiting for instance to start..."

# Wait for instance to be running
aws ec2 wait instance-running \
    --instance-ids "${INSTANCE_ID}" \
    --region "${AWS_REGION}"

echo "✓ Instance is running"
echo ""

# Get instance public IP
PUBLIC_IP=$(aws ec2 describe-instances \
    --instance-ids "${INSTANCE_ID}" \
    --region "${AWS_REGION}" \
    --query 'Reservations[0].Instances[0].PublicIpAddress' \
    --output text)

echo "✓ Public IP: ${PUBLIC_IP}"
echo ""

# Step 4: Wait for instance to be fully initialized
echo "Waiting for instance to be fully initialized (this may take 2-3 minutes)..."
sleep 60

# Wait for SSH to be available
echo "Waiting for SSH to be available..."
MAX_ATTEMPTS=30
ATTEMPT=0
while [ $ATTEMPT -lt $MAX_ATTEMPTS ]; do
    if ssh -o StrictHostKeyChecking=no -o ConnectTimeout=5 -i "${KEY_FILE}" "ec2-user@${PUBLIC_IP}" "echo SSH is ready" 2>/dev/null; then
        echo "✓ SSH is available"
        break
    fi
    ATTEMPT=$((ATTEMPT + 1))
    echo "  Attempt $ATTEMPT/$MAX_ATTEMPTS..."
    sleep 10
done

if [ $ATTEMPT -eq $MAX_ATTEMPTS ]; then
    echo "Error: SSH connection timeout"
    exit 1
fi
echo ""

# Step 5: Wait for user-data script to complete
echo "Waiting for setup script to complete..."
MAX_ATTEMPTS=30
ATTEMPT=0
while [ $ATTEMPT -lt $MAX_ATTEMPTS ]; do
    if ssh -o StrictHostKeyChecking=no -i "${KEY_FILE}" "ec2-user@${PUBLIC_IP}" "test -f /opt/redis-automerge/setup-complete.txt" 2>/dev/null; then
        echo "✓ Setup script completed"
        break
    fi
    ATTEMPT=$((ATTEMPT + 1))
    echo "  Attempt $ATTEMPT/$MAX_ATTEMPTS..."
    sleep 10
done

if [ $ATTEMPT -eq $MAX_ATTEMPTS ]; then
    echo "Warning: Setup script may still be running"
fi
echo ""

# Step 6: Deploy application
echo "Deploying application..."

if [ -n "${GIT_REPO}" ]; then
    # Deploy via git
    echo "Deploying from git repository..."
    ssh -o StrictHostKeyChecking=no -i "${KEY_FILE}" "ec2-user@${PUBLIC_IP}" << 'EOF'
        cd /opt/redis-automerge
        git clone ${GIT_REPO} app
        cd app
        docker-compose up -d
EOF
else
    # Deploy via SCP
    echo "Deploying via file copy..."

    # Copy project files (excluding unnecessary files)
    cd "${SCRIPT_DIR}/.."
    tar czf /tmp/redis-automerge.tar.gz \
        --exclude='target' \
        --exclude='.git' \
        --exclude='aws-deploy' \
        --exclude='node_modules' \
        .

    scp -o StrictHostKeyChecking=no -i "${KEY_FILE}" \
        /tmp/redis-automerge.tar.gz \
        "ec2-user@${PUBLIC_IP}:/tmp/"

    ssh -o StrictHostKeyChecking=no -i "${KEY_FILE}" "ec2-user@${PUBLIC_IP}" << 'EOF'
        cd /opt/redis-automerge
        sudo tar xzf /tmp/redis-automerge.tar.gz
        rm /tmp/redis-automerge.tar.gz

        # Build and start containers
        sudo docker-compose build
        sudo docker-compose up -d

        echo "Waiting for services to start..."
        sleep 10

        # Check if containers are running
        sudo docker-compose ps
EOF

    rm /tmp/redis-automerge.tar.gz
fi

echo "✓ Application deployed"
echo ""

# Step 7: Display connection information
echo "=========================================="
echo "Deployment Complete!"
echo "=========================================="
echo ""
echo "Instance ID: ${INSTANCE_ID}"
echo "Public IP: ${PUBLIC_IP}"
echo "SSH Key: ${KEY_FILE}"
echo ""
echo "Access your application:"
echo "  Demo Editor: http://${PUBLIC_IP}:${HTTP_PORT}/editor.html"
echo "  Main Demo:   http://${PUBLIC_IP}:${HTTP_PORT}/index.html"
echo ""
echo "SSH into instance:"
echo "  ssh -i ${KEY_FILE} ec2-user@${PUBLIC_IP}"
echo ""
echo "View logs:"
echo "  ssh -i ${KEY_FILE} ec2-user@${PUBLIC_IP}"
echo "  cd /opt/redis-automerge"
echo "  sudo docker-compose logs -f"
echo ""
echo "Manage instance:"
echo "  ./aws-deploy/manage.sh [start|stop|restart|status|ssh|logs]"
echo ""
echo "Teardown:"
echo "  ./aws-deploy/teardown.sh"
echo ""
echo "=========================================="

# Save instance info
cat > "${SCRIPT_DIR}/instance-info.txt" << EOF
INSTANCE_ID=${INSTANCE_ID}
PUBLIC_IP=${PUBLIC_IP}
REGION=${AWS_REGION}
KEY_FILE=${KEY_FILE}
DEPLOYED_AT="$(date)"
EOF

echo "Instance info saved to: ${SCRIPT_DIR}/instance-info.txt"
