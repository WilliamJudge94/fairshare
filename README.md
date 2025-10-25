# fairshare

**Fair resource allocation for shared Linux systems**

A lightweight resource manager that prevents any single user from monopolizing CPU and memory on multi-user Linux servers. fairshare automatically enforces per-user limits while allowing users to request additional resources dynamically when they need them.

### The Problem fairshare Solves

On shared Linux systems, one user running a heavy workload can consume all available CPU and memory, leaving nothing for other users. This creates a terrible experience and reduces productivity.

### The Solution

fairshare allocates resources fairly across all users:
- **Default**: Each user gets 1 CPU core and 2GB RAM by default
- **On-demand**: Users can request more resources when needed (up to 1000 CPU cores and 10000 GB RAM)
- **Automatic**: The system grants requests only if resources are truly available
- **Fair**: No user can monopolize the entire system

## Installation

### Quick Install (Recommended)
```bash
make release
```
This builds the release binary and installs it to `/usr/local/bin/fairshare`.

### Manual Build
If you prefer to build manually:
```bash
cargo build --release
sudo cp target/release/fairshare /usr/local/bin/
```

### Requirements
- **Linux** with systemd (including user session support)
- **Rust 1.70+** (only needed for building)

## Usage Guide

### User Commands

#### 1. Check System Status
See how much CPU and memory is available and what each user is currently using.
```bash
fairshare status
```

#### 2. Check Your Current Allocation
View how much CPU and memory your user session has access to.
```bash
fairshare info
```

#### 3. Request More Resources
Ask for additional CPU and memory. The system will grant your request only if resources are available.
```bash
fairshare request --cpu 4 --mem 8
```
This requests 4 CPU cores and 8GB of RAM.

**Constraints:**
- CPU: 1–1000 cores
- Memory: 1–10000 GB

#### 4. Release Resources
Return your resources to the system and revert to the default allocation (1 CPU core, 2GB RAM).
```bash
fairshare release
```

### Administrator Commands

> **Note:** These commands require root privileges (`sudo`)

#### Set Default Limits
Configure the default CPU and memory allocation for all users when they first log in.
```bash
sudo fairshare admin setup --cpu 1 --mem 2
```

#### Uninstall fairshare
Remove fairshare from your system and revert to standard Linux resource management.
```bash
sudo fairshare admin uninstall --force
```

## Troubleshooting

### Commands fail with "permission denied"
Make sure your systemd user session is running:
```bash
systemctl --user status
```

### Resource request fails
Check available resources on the system:
```bash
fairshare status
```
Your request may exceed the available resources.

### Changes don't take effect
After running `sudo fairshare admin setup`, systemd needs to reload:
```bash
sudo systemctl daemon-reload
```

## License

This project is open source.
