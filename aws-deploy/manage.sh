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
get_public_ip() {
    aws ec2 describe-instances \
        --instance-ids "${INSTANCE_ID}" \
        --region "${REGION}" \
        --query 'Reservations[0].Instances[0].PublicIpAddress' \
        --output text 2>/dev/null || echo "none"
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
        echo ""
        if [ "${STATE}" == "running" ]; then
            echo "Application URL: http://${IP}:8080/editor.html"
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
        echo "Application URL: http://${IP}:8080/editor.html"

        # Update instance info with new IP
        sed -i.bak "s/PUBLIC_IP=.*/PUBLIC_IP=${IP}/" "${SCRIPT_DIR}/instance-info.txt"
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
        echo "Application URL: http://${IP}:8080/editor.html"
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

        # Create tarball
        cd "${SCRIPT_DIR}/.."
        tar czf /tmp/redis-automerge.tar.gz \
            --exclude='target' \
            --exclude='.git' \
            --exclude='aws-deploy' \
            --exclude='node_modules' \
            .

        # Upload and deploy
        scp -o StrictHostKeyChecking=no -i "${KEY_FILE}" \
            /tmp/redis-automerge.tar.gz \
            "ec2-user@${IP}:/tmp/"

        ssh -o StrictHostKeyChecking=no -i "${KEY_FILE}" "ec2-user@${IP}" << 'EOF'
            cd /opt/redis-automerge

            # Stop containers
            sudo docker-compose down

            # Backup old files
            sudo mv docker-compose.yml docker-compose.yml.bak 2>/dev/null || true

            # Extract new files
            sudo tar xzf /tmp/redis-automerge.tar.gz
            rm /tmp/redis-automerge.tar.gz

            # Rebuild and restart
            sudo docker-compose build
            sudo docker-compose up -d

            echo "Waiting for services to start..."
            sleep 10

            sudo docker-compose ps
EOF

        rm /tmp/redis-automerge.tar.gz
        echo "✓ Application updated"
        echo "Application URL: http://${IP}:8080/editor.html"
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
        echo "SSH Key: ${KEY_FILE}"
        echo "Deployed: ${DEPLOYED_AT}"
        echo ""
        echo "URLs:"
        if [ "${STATE}" == "running" ]; then
            echo "  Demo Editor: http://${IP}:8080/editor.html"
            echo "  Main Demo:   http://${IP}:8080/index.html"
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
        echo "  (Free tier: 750 hours/month for 12 months)"
        ;;

    *)
        echo "Error: Unknown command '${COMMAND}'"
        echo ""
        show_usage
        exit 1
        ;;
esac
