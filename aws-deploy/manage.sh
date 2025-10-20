#!/bin/bash
# Instance management script for redis-automerge

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

# Function to show usage
show_usage() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  status    - Show instance status"
    echo "  start     - Start stopped instance"
    echo "  stop      - Stop running instance"
    echo "  restart   - Restart instance"
    echo "  ssh       - SSH into instance"
    echo "  logs      - View docker-compose logs"
    echo "  update    - Update and redeploy application"
    echo "  info      - Show instance information"
    echo ""
}

# Function to get instance state
get_instance_state() {
    aws ec2 describe-instances \
        --instance-ids "${INSTANCE_ID}" \
        --region "${REGION}" \
        --query 'Reservations[0].Instances[0].State.Name' \
        --output text 2>/dev/null || echo "unknown"
}

# Function to get public IP
# Prefers Elastic IP from instance-info.txt if available (static)
# Falls back to querying EC2 for dynamic IP
get_public_ip() {
    if [ -n "${ELASTIC_IP}" ]; then
        echo "${ELASTIC_IP}"
    else
        aws ec2 describe-instances \
            --instance-ids "${INSTANCE_ID}" \
            --region "${REGION}" \
            --query 'Reservations[0].Instances[0].PublicIpAddress' \
            --output text 2>/dev/null || echo "none"
    fi
}

# Main command handling
COMMAND="${1:-status}"

case "${COMMAND}" in
    status)
        echo "=========================================="
        echo "Instance Status"
        echo "=========================================="
        STATE=$(get_instance_state)
        IP=$(get_public_ip)
        echo "Instance ID: ${INSTANCE_ID}"
        echo "State: ${STATE}"
        echo "Public IP: ${IP}"

        if [ -n "${DOMAIN}" ]; then
            echo "Domain: ${DOMAIN}"
        fi

        echo ""
        if [ "${STATE}" == "running" ]; then
            if [ -n "${DOMAIN}" ]; then
                echo "Application URL: https://${DOMAIN}"
            else
                echo "Application URL: http://${IP}:8080/editor.html"
            fi
        fi
        ;;

    start)
        echo "Starting instance ${INSTANCE_ID}..."
        aws ec2 start-instances \
            --instance-ids "${INSTANCE_ID}" \
            --region "${REGION}"

        echo "Waiting for instance to start..."
        aws ec2 wait instance-running \
            --instance-ids "${INSTANCE_ID}" \
            --region "${REGION}"

        IP=$(get_public_ip)
        echo "✓ Instance started"
        echo "Public IP: ${IP}"

        if [ -n "${DOMAIN}" ]; then
            echo "Application URL: https://${DOMAIN}"
        else
            echo "Application URL: http://${IP}:8080/editor.html"
        fi

        if [ -n "${ELASTIC_IP}" ]; then
            echo "Note: Using Elastic IP - IP address persists across restarts"
        fi
        ;;

    stop)
        echo "Stopping instance ${INSTANCE_ID}..."
        aws ec2 stop-instances \
            --instance-ids "${INSTANCE_ID}" \
            --region "${REGION}"

        echo "Waiting for instance to stop..."
        aws ec2 wait instance-stopped \
            --instance-ids "${INSTANCE_ID}" \
            --region "${REGION}"

        echo "✓ Instance stopped"
        echo "Note: You are not charged for stopped instances (only for EBS storage)"
        ;;

    restart)
        echo "Restarting instance ${INSTANCE_ID}..."
        aws ec2 reboot-instances \
            --instance-ids "${INSTANCE_ID}" \
            --region "${REGION}"

        echo "✓ Instance restart initiated"
        echo "Waiting for instance to be accessible..."
        sleep 30

        IP=$(get_public_ip)
        if [ -n "${DOMAIN}" ]; then
            echo "Application URL: https://${DOMAIN}"
        else
            echo "Application URL: http://${IP}:8080/editor.html"
        fi
        ;;

    ssh)
        STATE=$(get_instance_state)
        if [ "${STATE}" != "running" ]; then
            echo "Error: Instance is not running (state: ${STATE})"
            exit 1
        fi

        IP=$(get_public_ip)
        echo "Connecting to ${IP}..."
        ssh -o StrictHostKeyChecking=no -i "${KEY_FILE}" "ec2-user@${IP}"
        ;;

    logs)
        STATE=$(get_instance_state)
        if [ "${STATE}" != "running" ]; then
            echo "Error: Instance is not running (state: ${STATE})"
            exit 1
        fi

        IP=$(get_public_ip)
        echo "Viewing logs from ${IP}..."
        ssh -o StrictHostKeyChecking=no -i "${KEY_FILE}" "ec2-user@${IP}" \
            "cd /opt/redis-automerge && sudo docker-compose logs -f"
        ;;

    update)
        STATE=$(get_instance_state)
        if [ "${STATE}" != "running" ]; then
            echo "Error: Instance is not running (state: ${STATE})"
            exit 1
        fi

        IP=$(get_public_ip)
        echo "Updating application on ${IP}..."

        # Check for production environment file
        cd "${SCRIPT_DIR}/.."
        if [ ! -f ".env.production" ]; then
            echo ""
            echo "⚠️  WARNING: .env.production not found!"
            echo "Create .env.production with your production OAuth credentials"
            echo "See .env.production template for details"
            echo ""
            exit 1
        fi

        # Copy production .env to temporary location
        cp .env.production /tmp/.env.production.tmp

        # Create tarball
        tar czf /tmp/redis-automerge.tar.gz \
            --exclude='target' \
            --exclude='.git' \
            --exclude='aws-deploy' \
            --exclude='node_modules' \
            --exclude='.env' \
            --exclude='.env.production' \
            .

        # Upload and deploy
        scp -o StrictHostKeyChecking=no -i "${KEY_FILE}" \
            /tmp/redis-automerge.tar.gz \
            "ec2-user@${IP}:/tmp/"

        # Copy production .env
        scp -o StrictHostKeyChecking=no -i "${KEY_FILE}" \
            /tmp/.env.production.tmp \
            "ec2-user@${IP}:/tmp/.env.production"

        ssh -o StrictHostKeyChecking=no -i "${KEY_FILE}" "ec2-user@${IP}" << 'EOF'
            cd /opt/redis-automerge

            # Stop containers
            sudo docker-compose down

            # Backup old files
            sudo mv docker-compose.yml docker-compose.yml.bak 2>/dev/null || true

            # Extract new files
            sudo tar xzf /tmp/redis-automerge.tar.gz
            rm /tmp/redis-automerge.tar.gz

            # Copy production .env as .env
            sudo cp /tmp/.env.production .env
            sudo chown ec2-user:ec2-user .env
            rm /tmp/.env.production

            # Rebuild and restart with production config
            sudo docker-compose -f docker-compose.yml -f docker-compose.prod.yml build
            sudo docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d

            echo "Waiting for services to start..."
            sleep 10

            sudo docker-compose -f docker-compose.yml -f docker-compose.prod.yml ps
EOF

        rm /tmp/redis-automerge.tar.gz
        rm /tmp/.env.production.tmp
        echo "✓ Application updated"

        if [ -n "${DOMAIN}" ]; then
            echo "Application URL: https://${DOMAIN}"
        else
            echo "Application URL: http://${IP}:8080/editor.html"
        fi
        ;;

    info)
        echo "=========================================="
        echo "Instance Information"
        echo "=========================================="
        STATE=$(get_instance_state)
        IP=$(get_public_ip)
        echo "Instance ID: ${INSTANCE_ID}"
        echo "Region: ${REGION}"
        echo "State: ${STATE}"
        echo "Public IP: ${IP}"

        if [ -n "${ELASTIC_IP}" ]; then
            echo "Elastic IP: ${ELASTIC_IP}"
            echo "  Allocation ID: ${ALLOCATION_ID}"
        fi

        if [ -n "${DOMAIN}" ]; then
            echo "Domain: ${DOMAIN}"
            if [ -n "${HOSTED_ZONE_ID}" ]; then
                echo "  Hosted Zone: ${HOSTED_ZONE_ID}"
            fi
        fi

        echo "SSH Key: ${KEY_FILE}"
        echo "Deployed: ${DEPLOYED_AT}"
        echo ""
        echo "URLs:"
        if [ "${STATE}" == "running" ]; then
            if [ -n "${DOMAIN}" ]; then
                echo "  Production (HTTPS):      https://${DOMAIN}"
                echo "  Production (HTTP redirect): http://${DOMAIN}"
                echo "  Dev (HTTP):              http://${DOMAIN}:8080/editor.html"
            else
                echo "  Demo Editor: http://${IP}:8080/editor.html"
                echo "  Main Demo:   http://${IP}:8080/index.html"
            fi
        else
            echo "  (Instance not running)"
        fi
        echo ""
        echo "SSH Command:"
        echo "  ssh -i ${KEY_FILE} ec2-user@${IP}"
        echo ""
        echo "Cost Estimate:"
        echo "  t3.micro: ~$0.01/hour (~$7.50/month)"
        echo "  EBS Storage: ~$0.10/GB/month"
        if [ -n "${ELASTIC_IP}" ]; then
            echo "  Elastic IP: Free while attached, $0.005/hour if unattached"
        fi
        echo "  (Free tier: 750 hours/month for 12 months)"
        ;;

    *)
        echo "Error: Unknown command '${COMMAND}'"
        echo ""
        show_usage
        exit 1
        ;;
esac
