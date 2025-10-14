# AWS Deployment Guide

This directory contains scripts to deploy redis-automerge to AWS EC2 with a single command.

## Overview

- **Cost**: ~$0-8/month (t3.micro free tier eligible for 12 months)
- **Setup Time**: ~5 minutes
- **Management**: Simple CLI commands

## Prerequisites

### 1. Install AWS CLI

**macOS:**
```bash
brew install awscli
```

**Linux:**
```bash
curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip"
unzip awscliv2.zip
sudo ./aws/install
```

**Verify:**
```bash
aws --version
```

### 2. Configure AWS Credentials

```bash
aws configure
```

You'll need:
- AWS Access Key ID
- AWS Secret Access Key
- Default region (e.g., `us-west-2`)
- Default output format (`json`)

**Get credentials:**
1. Go to [AWS Console](https://console.aws.amazon.com/)
2. Navigate to IAM → Users → Your User → Security Credentials
3. Create Access Key

### 3. Check Your AWS Account

```bash
aws sts get-caller-identity
```

Should show your account info.

## Quick Start

### Deploy (One Command)

```bash
./aws-deploy/deploy.sh
```

This will:
1. ✓ Create SSH key pair
2. ✓ Create security group with firewall rules
3. ✓ Launch EC2 instance (t3.micro)
4. ✓ Install Docker & Docker Compose
5. ✓ Deploy your application
6. ✓ Display connection info

**Deployment takes ~3-5 minutes**

### Access Your Application

After deployment, you'll see:

```
Application URL: http://YOUR-IP:8080/editor.html
SSH Command: ssh -i ~/.ssh/redis-automerge-key.pem ec2-user@YOUR-IP
```

Open the URL in your browser to access the collaborative editor!

## Configuration

Edit `aws-deploy/aws-config.sh` to customize:

```bash
# AWS Region
export AWS_REGION="us-west-2"

# Instance type (free tier eligible)
export INSTANCE_TYPE="t3.micro"

# Instance name
export INSTANCE_NAME="redis-automerge"

# Ports
export HTTP_PORT=8080
export REDIS_PORT=6379
export WEBDIS_PORT=7379
```

## Management Commands

### Check Status
```bash
./aws-deploy/manage.sh status
```

### Stop Instance (Save Money)
```bash
./aws-deploy/manage.sh stop
```
- Stops billing for compute (only pay for storage: ~$0.30/month)
- Data is preserved
- Can restart anytime

### Start Instance
```bash
./aws-deploy/manage.sh start
```
- Restarts stopped instance
- May get a new IP address

### SSH Into Instance
```bash
./aws-deploy/manage.sh ssh
```

### View Application Logs
```bash
./aws-deploy/manage.sh logs
```

### Update Application
```bash
./aws-deploy/manage.sh update
```
- Uploads your latest code
- Rebuilds containers
- Restarts services

### Restart Instance
```bash
./aws-deploy/manage.sh restart
```

### Show Full Info
```bash
./aws-deploy/manage.sh info
```

## Teardown (Delete Everything)

**⚠️ This will delete all resources and data!**

```bash
./aws-deploy/teardown.sh
```

Removes:
- EC2 instance
- Security group
- SSH key pair (from AWS)

Local SSH key file is preserved unless you choose to delete it.

## Cost Breakdown

### Free Tier (First 12 Months)
- **t3.micro**: 750 hours/month FREE
- **EBS Storage**: 30 GB FREE
- **Data Transfer**: 15 GB/month FREE

### After Free Tier
- **t3.micro**: ~$7.50/month ($0.0104/hour)
- **EBS Storage**: ~$1/month (10 GB @ $0.10/GB)
- **Data Transfer**: First 100 GB FREE/month

**Total: ~$8.50/month after free tier**

### To Save Money
```bash
# Stop when not using (nights/weekends)
./aws-deploy/manage.sh stop

# Only pay for storage (~$1/month)
# Restart when needed
./aws-deploy/manage.sh start
```

## Troubleshooting

### Deployment Fails

1. **Check AWS credentials:**
   ```bash
   aws sts get-caller-identity
   ```

2. **Check region has t3.micro:**
   ```bash
   aws ec2 describe-instance-types --instance-types t3.micro --region us-west-2
   ```

3. **View deployment logs:**
   - SSH into instance (if it started)
   - Check: `sudo cat /var/log/user-data.log`

### Can't Connect to Application

1. **Check instance is running:**
   ```bash
   ./aws-deploy/manage.sh status
   ```

2. **Check security group allows port 8080:**
   ```bash
   aws ec2 describe-security-groups --group-names redis-automerge-sg
   ```

3. **Check containers are running:**
   ```bash
   ./aws-deploy/manage.sh ssh
   cd /opt/redis-automerge
   sudo docker-compose ps
   ```

### SSH Connection Refused

- Wait 2-3 minutes after deployment
- Instance may still be initializing

### Application Not Responding

1. **View container logs:**
   ```bash
   ./aws-deploy/manage.sh logs
   ```

2. **Restart containers:**
   ```bash
   ./aws-deploy/manage.sh ssh
   cd /opt/redis-automerge
   sudo docker-compose restart
   ```

## Security Notes

### Firewall (Security Group)

By default, the deployment opens:
- **Port 22**: SSH (from anywhere)
- **Port 8080**: HTTP (from anywhere)

Redis (6379) and Webdis (7379) are **NOT exposed** externally (only accessible within Docker network).

### To Restrict SSH Access

Edit security group after deployment:

```bash
# Get your IP
MY_IP=$(curl -s ifconfig.me)

# Update SSH rule to only allow your IP
aws ec2 authorize-security-group-ingress \
  --group-name redis-automerge-sg \
  --protocol tcp \
  --port 22 \
  --cidr ${MY_IP}/32
```

### HTTPS Setup (Optional)

To add HTTPS with Let's Encrypt:

1. Get a domain name (e.g., from Route 53 or Namecheap)
2. Point domain to your instance IP
3. Edit `aws-config.sh` to set `DOMAIN`
4. Uncomment HTTPS setup in `setup.sh`
5. Redeploy or update

## Advanced Usage

### Use Git for Deployment

Instead of copying files via SCP:

1. Edit `aws-config.sh`:
   ```bash
   export GIT_REPO="https://github.com/yourusername/redis-automerge.git"
   ```

2. Deploy:
   ```bash
   ./aws-deploy/deploy.sh
   ```

Instance will clone from git instead of copying local files.

### Change Instance Type

To upgrade to more powerful instance:

1. Stop instance:
   ```bash
   ./aws-deploy/manage.sh stop
   ```

2. Change instance type:
   ```bash
   aws ec2 modify-instance-attribute \
     --instance-id i-xxxxx \
     --instance-type t3.small
   ```

3. Start instance:
   ```bash
   ./aws-deploy/manage.sh start
   ```

### Backup Data

Redis data is stored in Docker volumes. To backup:

```bash
./aws-deploy/manage.sh ssh
cd /opt/redis-automerge
sudo docker-compose exec redis redis-cli SAVE
sudo docker cp redis-automerge-redis-1:/data/dump.rdb ./backup.rdb
exit

# Download backup
scp -i ~/.ssh/redis-automerge-key.pem ec2-user@YOUR-IP:/opt/redis-automerge/backup.rdb ./
```

## Architecture

```
[Your Computer]
     |
     | (SSH / HTTP)
     |
[EC2 Instance: t3.micro]
     |
     +-- Docker Compose
          |
          +-- Redis Container (port 6379)
          |   +-- redis-automerge module
          |
          +-- Webdis Container (port 7379)
          |   +-- HTTP/WebSocket API
          |
          +-- Nginx Container (port 8080)
              +-- Static files (demo UI)
```

## Files

- **aws-config.sh**: Configuration (edit this)
- **deploy.sh**: Main deployment script
- **manage.sh**: Management commands
- **teardown.sh**: Cleanup script
- **setup.sh**: User data (runs on instance boot)
- **instance-info.txt**: Created after deployment (instance details)

## Getting Help

1. Check AWS costs: [AWS Billing Dashboard](https://console.aws.amazon.com/billing/)
2. EC2 Console: [EC2 Instances](https://console.aws.amazon.com/ec2/v2/home#Instances)
3. View this guide: `cat aws-deploy/README.md`

## Next Steps

After deployment:

1. **Test the application**: Open the URL in your browser
2. **Create a room**: Switch to "Shareable Link Mode"
3. **Share with others**: Copy the room link
4. **Monitor costs**: Check AWS billing regularly
5. **Stop when not using**: Save money by stopping the instance

## Alternative: Lightsail

If you prefer simpler management with fixed pricing ($5/month), see the Lightsail deployment option in the main project README.
