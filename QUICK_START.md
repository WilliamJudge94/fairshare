# Fairshare Daemon - Quick Start Guide

## What is fairshared?

`fairshared` is a system daemon that manages CPU and memory resources for users through systemd cgroups. It enforces resource limits defined in policy files and provides a simple IPC interface for resource allocation requests.

## Prerequisites

- Linux system with systemd
- Rust toolchain (1.70+)
- DBus system access
- Root privileges (for systemd slice management)

## Installation

### Build from source

```bash
# Clone the repository
git clone <repository-url>
cd fairshare

# Build the release binary
cargo build --release

# The binary will be at: target/release/fairshared
```

### Quick test build

```bash
cargo build
# Binary at: target/debug/fairshared
```

## Configuration

### 1. Create a policy file

Create `/etc/fairshare/policy.d/default.yaml`:

```yaml
defaults:
  cpu: 2
  mem: 8G

max:
  cpu: 8
  mem: 32G
```

This defines:
- Default allocation: 2 CPUs, 8GB RAM
- Maximum allowed: 8 CPUs, 32GB RAM

### 2. Create policy directory

```bash
sudo mkdir -p /etc/fairshare/policy.d/
sudo cp examples/default.yaml /etc/fairshare/policy.d/default.yaml
```

## Running the Daemon

### Basic usage

```bash
# Using default paths
sudo ./target/release/fairshared

# With custom policy and socket
sudo ./target/release/fairshared /path/to/policy.yaml /path/to/socket.sock
```

### Default paths
- Policy: `/etc/fairshare/policy.d/default.yaml`
- Socket: `/run/fairshare.sock`

### Enable debug logging

```bash
RUST_LOG=debug sudo ./target/release/fairshared
```

Log levels: `error`, `warn`, `info`, `debug`, `trace`

## Testing the Daemon

### 1. Start daemon with test policy

```bash
# Use the included test policy
sudo ./target/release/fairshared test_policy.yaml /tmp/fairshare-test.sock
```

### 2. Send test request (using netcat or custom client)

```bash
# Request resources
echo '{"type":"request_resources","cpu":4,"mem":"16G"}' | nc -U /tmp/fairshare-test.sock

# Check status
echo '{"type":"status"}' | nc -U /tmp/fairshare-test.sock

# Release resources
echo '{"type":"release"}' | nc -U /tmp/fairshare-test.sock
```

### 3. Verify systemd slice created

```bash
# Should show fairshare-{UID}.slice
systemctl status fairshare-$(id -u).slice
```

## IPC Protocol

### Request Types

#### 1. Request Resources
```json
{
  "type": "request_resources",
  "cpu": 4,
  "mem": "16G"
}
```

#### 2. Release Resources
```json
{
  "type": "release"
}
```

#### 3. Check Status
```json
{
  "type": "status"
}
```

### Response Types

#### Success
```json
{
  "type": "success",
  "message": "Resources allocated: 4 CPUs, 16G memory"
}
```

#### Error
```json
{
  "type": "error",
  "error": "Requested CPU (16) exceeds maximum allowed (8)"
}
```

#### Status Info
```json
{
  "type": "status_info",
  "allocated_cpu": 4,
  "allocated_mem": "16G"
}
```

## Memory Unit Format

Supported units:
- `B` - Bytes
- `K`, `KB` - Kilobytes (1024 bytes)
- `M`, `MB` - Megabytes (1024^2 bytes)
- `G`, `GB` - Gigabytes (1024^3 bytes)
- `T`, `TB` - Terabytes (1024^4 bytes)

Examples: `8G`, `512M`, `16GB`, `1.5G`

## Common Errors and Solutions

### "Policy file not found"
```bash
# Create the policy directory and file
sudo mkdir -p /etc/fairshare/policy.d/
sudo nano /etc/fairshare/policy.d/default.yaml
# Add policy content (see Configuration above)
```

### "Failed to connect to system DBus"
```bash
# Ensure DBus is running
systemctl status dbus

# Check DBus permissions
# Daemon needs access to org.freedesktop.systemd1
```

### "Failed to bind Unix socket"
```bash
# Check if socket already exists
ls -l /run/fairshare.sock

# Remove stale socket
sudo rm /run/fairshare.sock

# Or use a different path
sudo ./fairshared /etc/fairshare/policy.d/default.yaml /tmp/fairshare.sock
```

### "Permission denied"
```bash
# Daemon requires root privileges for systemd management
sudo ./target/release/fairshared
```

### "User already has an active resource allocation"
```bash
# Release existing allocation first
echo '{"type":"release"}' | nc -U /run/fairshare.sock

# Then request new resources
echo '{"type":"request_resources","cpu":4,"mem":"16G"}' | nc -U /run/fairshare.sock
```

## Development and Testing

### Run tests

```bash
# Run all unit tests
cargo test

# Run with verbose output
cargo test -- --nocapture

# Run integration tests (requires systemd)
cargo test -- --ignored

# Run specific test
cargo test test_policy_parsing
```

### Build with debug info

```bash
cargo build
RUST_BACKTRACE=1 sudo ./target/debug/fairshared
```

### Check for unused dependencies

```bash
cargo install cargo-udeps
cargo +nightly udeps
```

## Production Deployment

### Systemd Service (example)

Create `/etc/systemd/system/fairshared.service`:

```ini
[Unit]
Description=Fairshare Resource Management Daemon
After=dbus.service
Requires=dbus.service

[Service]
Type=simple
ExecStart=/usr/local/bin/fairshared
Restart=on-failure
RestartSec=5s

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/run

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable fairshared
sudo systemctl start fairshared
sudo systemctl status fairshared
```

### Log Rotation

Create `/etc/logrotate.d/fairshared`:

```
/var/log/fairshared.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 0644 root root
}
```

## Architecture Overview

```
┌─────────────┐
│   Client    │
└──────┬──────┘
       │ JSON over Unix Socket
       │
┌──────▼──────┐
│ IPC Server  │
└──────┬──────┘
       │
┌──────▼──────────┐
│  Daemon Core    │
│  - Request      │
│    Handler      │
│  - Policy       │
│    Validator    │
│  - Allocation   │
│    Tracker      │
└──────┬──────────┘
       │
┌──────▼──────────┐
│ Systemd Client  │
│   (DBus API)    │
└──────┬──────────┘
       │
┌──────▼──────────┐
│    systemd      │
│  Slice Manager  │
└─────────────────┘
```

## Monitoring

### View logs

```bash
# With systemd
journalctl -u fairshared -f

# Direct run
# Logs go to stdout with structured format
```

### Check active slices

```bash
# List all fairshare slices
systemctl list-units 'fairshare-*.slice'

# Check specific slice
systemctl status fairshare-1000.slice

# View slice properties
systemctl show fairshare-1000.slice
```

### Monitor resource usage

```bash
# View cgroup stats
systemd-cgtop

# Specific slice
systemctl status fairshare-1000.slice | grep -A 5 "Memory\|CPU"
```

## Troubleshooting

### Enable maximum logging

```bash
RUST_LOG=trace,fairshare=trace sudo ./target/release/fairshared
```

### Test DBus connectivity

```bash
# List systemd units via DBus (tests connection)
busctl --system call \
  org.freedesktop.systemd1 \
  /org/freedesktop/systemd1 \
  org.freedesktop.systemd1.Manager \
  ListUnits
```

### Validate policy file

```bash
# Check YAML syntax
yamllint /etc/fairshare/policy.d/default.yaml

# Test policy loading
RUST_LOG=debug sudo ./target/release/fairshared 2>&1 | grep -i policy
```

## Getting Help

1. Check logs for error messages
2. Review IMPLEMENTATION.md for technical details
3. Run tests: `cargo test`
4. Enable debug logging: `RUST_LOG=debug`
5. Check systemd status: `systemctl status`

## License

See LICENSE file in repository.

## Contributing

See CONTRIBUTING.md in repository.
