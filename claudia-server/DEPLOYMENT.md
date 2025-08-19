# Claudia Server Deployment Guide

This guide covers various deployment scenarios for Claudia Server, from local development to production environments.

## Table of Contents

- [Local Development](#local-development)
- [Production Deployment](#production-deployment)
- [Docker Deployment](#docker-deployment)
- [Systemd Service](#systemd-service)
- [Reverse Proxy Setup](#reverse-proxy-setup)
- [Security Considerations](#security-considerations)
- [Monitoring and Logging](#monitoring-and-logging)

## Local Development

### Quick Start

1. **Build and run locally:**
   ```bash
   cd claudia-server
   cargo build --release
   ./target/release/claudia-server
   ```

2. **Run with debug logging:**
   ```bash
   RUST_LOG=debug ./target/release/claudia-server --host 0.0.0.0 --port 3030
   ```

3. **Test the connection:**
   ```bash
   curl http://localhost:3030/health
   ```

### Development Configuration

Create a `config.toml` in your data directory (`~/.claudia-server/config.toml`):

```toml
[server]
max_concurrent_sessions = 5
session_timeout_seconds = 1800
auto_cleanup = true
cleanup_interval_seconds = 180

[claude]
default_model = "claude-3-5-sonnet-20241022"
verbose = true
skip_permissions = true

[process]
max_output_lines = 500
save_output_to_disk = true

[logging]
level = "debug"
log_to_file = true
```

## Production Deployment

### Prerequisites

1. **Claude CLI installed and accessible**
2. **Sufficient system resources** (2GB+ RAM recommended)
3. **Network access** for Claude API calls
4. **User with appropriate permissions**

### Build for Production

```bash
# Build optimized release
cargo build --release --target x86_64-unknown-linux-gnu

# Strip debug symbols to reduce size
strip target/x86_64-unknown-linux-gnu/release/claudia-server
```

### Installation

```bash
# Create dedicated user
sudo useradd --system --shell /bin/false claudia-server

# Create directories
sudo mkdir -p /opt/claudia-server
sudo mkdir -p /var/lib/claudia-server
sudo mkdir -p /var/log/claudia-server

# Copy binary
sudo cp target/release/claudia-server /opt/claudia-server/
sudo chown claudia-server:claudia-server /opt/claudia-server/claudia-server
sudo chmod 755 /opt/claudia-server/claudia-server

# Set permissions
sudo chown -R claudia-server:claudia-server /var/lib/claudia-server
sudo chown -R claudia-server:claudia-server /var/log/claudia-server
```

### Production Configuration

Create `/var/lib/claudia-server/config.toml`:

```toml
[server]
max_concurrent_sessions = 20
session_timeout_seconds = 3600
auto_cleanup = true
cleanup_interval_seconds = 300

[claude]
default_model = "claude-3-5-sonnet-20241022"
verbose = false
skip_permissions = true

[process]
max_output_lines = 1000
save_output_to_disk = true
output_directory = "sessions"

[logging]
level = "info"
log_to_file = true
log_file = "/var/log/claudia-server/claudia-server.log"
```

## Docker Deployment

### Dockerfile

Create `Dockerfile`:

```dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

# Install dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    nodejs \
    npm \
    && rm -rf /var/lib/apt/lists/*

# Install Claude CLI (adjust for your setup)
RUN npm install -g @anthropic-ai/claude-cli

# Create user
RUN useradd --create-home --shell /bin/bash claudia

# Copy binary
COPY --from=builder /app/target/release/claudia-server /usr/local/bin/
RUN chmod +x /usr/local/bin/claudia-server

# Create directories
RUN mkdir -p /data /logs
RUN chown -R claudia:claudia /data /logs

USER claudia
WORKDIR /home/claudia

EXPOSE 3030

CMD ["claudia-server", "--host", "0.0.0.0", "--port", "3030", "--data-dir", "/data"]
```

### Docker Compose

Create `docker-compose.yml`:

```yaml
version: '3.8'

services:
  claudia-server:
    build: .
    ports:
      - "3030:3030"
    volumes:
      - claudia_data:/data
      - claudia_logs:/logs
      - ./config.toml:/data/config.toml:ro
    environment:
      - RUST_LOG=info
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3030/health"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  claudia_data:
  claudia_logs:
```

### Run with Docker

```bash
# Build and start
docker-compose up -d

# View logs
docker-compose logs -f claudia-server

# Stop
docker-compose down
```

## Systemd Service

### Service File

Create `/etc/systemd/system/claudia-server.service`:

```ini
[Unit]
Description=Claudia Server - Claude Code API Wrapper
After=network.target
Wants=network.target

[Service]
Type=simple
User=claudia-server
Group=claudia-server
WorkingDirectory=/var/lib/claudia-server
ExecStart=/opt/claudia-server/claudia-server \
    --host 127.0.0.1 \
    --port 3030 \
    --data-dir /var/lib/claudia-server \
    --config /var/lib/claudia-server/config.toml

# Restart configuration
Restart=always
RestartSec=5
StartLimitInterval=60
StartLimitBurst=3

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/claudia-server /var/log/claudia-server

# Environment
Environment=RUST_LOG=info

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=claudia-server

[Install]
WantedBy=multi-user.target
```

### Service Management

```bash
# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable claudia-server
sudo systemctl start claudia-server

# Check status
sudo systemctl status claudia-server

# View logs
sudo journalctl -u claudia-server -f

# Restart service
sudo systemctl restart claudia-server
```

## Reverse Proxy Setup

### Nginx

Create `/etc/nginx/sites-available/claudia-server`:

```nginx
upstream claudia_server {
    server 127.0.0.1:3030;
}

server {
    listen 80;
    server_name your-domain.com;

    # Redirect HTTP to HTTPS
    return 301 https://$server_name$request_uri;
}

server {
    listen 443 ssl http2;
    server_name your-domain.com;

    # SSL configuration
    ssl_certificate /path/to/your/certificate.pem;
    ssl_certificate_key /path/to/your/private.key;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-RSA-AES256-GCM-SHA512:DHE-RSA-AES256-GCM-SHA512:ECDHE-RSA-AES256-GCM-SHA384:DHE-RSA-AES256-GCM-SHA384;

    # General proxy settings
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;

    # API endpoints
    location /api/ {
        proxy_pass http://claudia_server;
        proxy_read_timeout 300s;
        proxy_connect_timeout 75s;
    }

    # WebSocket endpoint
    location /ws {
        proxy_pass http://claudia_server;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 86400;
    }

    # Health check and info
    location ~ ^/(health|info)$ {
        proxy_pass http://claudia_server;
    }

    # Rate limiting
    limit_req_zone $binary_remote_addr zone=api:10m rate=10r/m;
    
    location /api/sessions {
        limit_req zone=api burst=5 nodelay;
        proxy_pass http://claudia_server;
    }
}
```

Enable the configuration:

```bash
sudo ln -s /etc/nginx/sites-available/claudia-server /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx
```

### Apache

Create `/etc/apache2/sites-available/claudia-server.conf`:

```apache
<VirtualHost *:80>
    ServerName your-domain.com
    Redirect permanent / https://your-domain.com/
</VirtualHost>

<VirtualHost *:443>
    ServerName your-domain.com
    
    # SSL Configuration
    SSLEngine on
    SSLCertificateFile /path/to/your/certificate.pem
    SSLCertificateKeyFile /path/to/your/private.key
    
    # Proxy settings
    ProxyPreserveHost On
    ProxyRequests Off
    
    # API endpoints
    ProxyPass /api/ http://127.0.0.1:3030/api/
    ProxyPassReverse /api/ http://127.0.0.1:3030/api/
    
    # WebSocket endpoint
    ProxyPass /ws ws://127.0.0.1:3030/ws
    ProxyPassReverse /ws ws://127.0.0.1:3030/ws
    
    # Health and info
    ProxyPass /health http://127.0.0.1:3030/health
    ProxyPassReverse /health http://127.0.0.1:3030/health
    ProxyPass /info http://127.0.0.1:3030/info
    ProxyPassReverse /info http://127.0.0.1:3030/info
    
    # Logging
    ErrorLog ${APACHE_LOG_DIR}/claudia-server_error.log
    CustomLog ${APACHE_LOG_DIR}/claudia-server_access.log combined
</VirtualHost>
```

## Security Considerations

### Network Security

1. **Bind to localhost only** in production:
   ```bash
   claudia-server --host 127.0.0.1 --port 3030
   ```

2. **Use a reverse proxy** for external access with HTTPS

3. **Implement rate limiting** to prevent abuse

4. **Configure firewall rules**:
   ```bash
   # Allow only reverse proxy access
   sudo ufw allow from 127.0.0.1 to any port 3030
   sudo ufw deny 3030
   ```

### Authentication

Since Claudia Server doesn't include built-in authentication, consider:

1. **VPN access** for internal use
2. **Basic auth in reverse proxy**:
   ```nginx
   location /api/ {
       auth_basic "Claudia Server";
       auth_basic_user_file /etc/nginx/.htpasswd;
       proxy_pass http://claudia_server;
   }
   ```

3. **OAuth proxy** like oauth2-proxy
4. **API Gateway** with authentication

### File System Security

1. **Run as dedicated user** with minimal permissions
2. **Restrict data directory access**:
   ```bash
   sudo chmod 750 /var/lib/claudia-server
   sudo chown claudia-server:claudia-server /var/lib/claudia-server
   ```

3. **Validate project paths** to prevent directory traversal
4. **Use AppArmor or SELinux** for additional isolation

## Monitoring and Logging

### Log Configuration

```toml
[logging]
level = "info"
log_to_file = true
log_file = "/var/log/claudia-server/claudia-server.log"
```

### Log Rotation

Create `/etc/logrotate.d/claudia-server`:

```
/var/log/claudia-server/*.log {
    daily
    missingok
    rotate 30
    compress
    delaycompress
    notifempty
    copytruncate
    postrotate
        systemctl reload claudia-server
    endscript
}
```

### Monitoring Script

Create a simple monitoring script:

```bash
#!/bin/bash
# claudia-server-monitor.sh

SERVER_URL="http://localhost:3030"
WEBHOOK_URL="your-slack-webhook-url"  # Optional

check_health() {
    response=$(curl -s -w "%{http_code}" "$SERVER_URL/health" -o /dev/null)
    if [ "$response" = "200" ]; then
        return 0
    else
        return 1
    fi
}

send_alert() {
    local message="$1"
    echo "[$(date)] ALERT: $message" >> /var/log/claudia-server/monitor.log
    
    # Optional: Send to Slack
    if [ -n "$WEBHOOK_URL" ]; then
        curl -X POST -H 'Content-type: application/json' \
            --data "{\"text\":\"Claudia Server Alert: $message\"}" \
            "$WEBHOOK_URL"
    fi
}

if ! check_health; then
    send_alert "Claudia Server health check failed"
    # Optionally restart service
    # sudo systemctl restart claudia-server
fi
```

Add to crontab:

```bash
# Check every 5 minutes
*/5 * * * * /usr/local/bin/claudia-server-monitor.sh
```

### Metrics Collection

For production monitoring, consider integrating with:

- **Prometheus** for metrics collection
- **Grafana** for visualization
- **ELK Stack** for log analysis
- **DataDog** or similar APM tools

### Performance Tuning

1. **Adjust concurrent session limits** based on available resources
2. **Configure cleanup intervals** to manage memory usage
3. **Monitor CPU and memory usage** of Claude processes
4. **Use SSD storage** for session data if possible
5. **Consider horizontal scaling** with load balancing for high traffic

## Troubleshooting

### Common Issues

1. **Claude binary not found**:
   ```bash
   # Check Claude installation
   which claude
   # Use custom path
   claudia-server --claude-path /custom/path/to/claude
   ```

2. **Permission denied errors**:
   ```bash
   # Check file permissions
   ls -la /var/lib/claudia-server
   # Fix ownership
   sudo chown -R claudia-server:claudia-server /var/lib/claudia-server
   ```

3. **Port already in use**:
   ```bash
   # Find process using port
   sudo lsof -i :3030
   # Use different port
   claudia-server --port 3031
   ```

4. **WebSocket connection fails**:
   - Check reverse proxy WebSocket configuration
   - Verify firewall rules
   - Test direct connection: `wscat -c ws://localhost:3030/ws`

### Debug Mode

Run with debug logging:

```bash
RUST_LOG=debug claudia-server --host 127.0.0.1 --port 3030
```

### Health Checks

Regular health check endpoints:

```bash
# Basic health
curl http://localhost:3030/health

# Detailed server info
curl http://localhost:3030/info

# Process statistics
curl http://localhost:3030/api/processes/stats
```

This deployment guide should cover most scenarios for running Claudia Server in various environments. Adjust the configurations based on your specific requirements and infrastructure.