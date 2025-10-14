#!/bin/bash
# AWS Deployment Configuration
# Edit these values for your deployment

# AWS Region
export AWS_REGION="us-west-2"

# EC2 Instance Configuration
export INSTANCE_TYPE="t3.micro"  # Free tier eligible
export INSTANCE_NAME="redis-automerge"

# AMI ID (Amazon Linux 2023 - update for your region)
# Find latest: aws ec2 describe-images --owners amazon --filters "Name=name,Values=al2023-ami-2023*" --query 'Images[0].ImageId' --output text --region us-west-2
export AMI_ID="ami-0a38c1c38a15fed74"  # Amazon Linux 2023 us-west-2

# SSH Key Configuration
export KEY_NAME="redis-automerge-key"
export KEY_FILE="$HOME/.ssh/${KEY_NAME}.pem"

# Security Group Configuration
export SECURITY_GROUP_NAME="redis-automerge-sg"

# Ports to open
export HTTP_PORT=8080
export REDIS_PORT=6379
export WEBDIS_PORT=7379

# Git Repository (optional - for auto-deployment)
# Leave empty to copy files via SCP instead
export GIT_REPO=""
# export GIT_REPO="https://github.com/yourusername/redis-automerge.git"

# Domain/DNS (optional)
export DOMAIN=""  # e.g., "demo.example.com"

# Tags
export TAGS="ResourceType=instance,Tags=[{Key=Name,Value=${INSTANCE_NAME}},{Key=Project,Value=redis-automerge}]"
