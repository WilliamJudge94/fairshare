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
- **PolicyKit (polkit)** for privilege escalation
- **Rust 1.70+** (only needed for building)

## Usage Guide

### User Commands

All user commands require `pkexec` for privilege escalation via PolicyKit. This allows you to manage your own resources without needing full administrator access.

#### 1. Check System Status
See how much CPU and memory is available and what each user is currently using.
```bash
pkexec fairshare status
```

#### 2. Check Your Current Allocation
View how much CPU and memory your user session has access to.
```bash
pkexec fairshare info
```

#### 3. Request Resources
Ask for CPU and memory resources. The system uses **smart delta-based checking** - it only needs enough free resources to cover the increase from your current allocation.

```bash
pkexec fairshare request --cpu 4 --mem 8
```
This requests 4 CPU cores and 8GB of RAM.

**Smart Allocation Example:**
- You currently have: 9GB RAM allocated
- System has: 2GB free
- You request: 10GB RAM
- Result: **SUCCESS** (net increase is only 1GB, which fits in the 2GB available)

**Constraints:**
- CPU: 1–1000 cores
- Memory: 1–10000 GB

#### 4. Release Resources
Return your resources to the system and revert to the default allocation (1 CPU core, 2GB RAM).
```bash
pkexec fairshare release
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

## How It Works

fairshare uses systemd as the authoritative source of truth for all resource allocations:

1. **Resource Tracking**: All allocations are stored directly in systemd user slices (`user-{UID}.slice`)
2. **Dynamic Querying**: The system queries systemd in real-time to get current allocations (no persistent state file)
3. **Delta-Based Checking**: When you request resources, fairshare calculates the net change from your current allocation
4. **Privilege Escalation**: Uses pkexec (PolicyKit) to allow users to modify their own slices without full root access

## Troubleshooting

### Commands fail with "authentication required" or "permission denied"
Make sure you're using `pkexec` for user commands:
```bash
pkexec fairshare status
```

If PolicyKit authentication keeps prompting, verify that admin setup completed successfully:
```bash
sudo fairshare admin setup --cpu 1 --mem 2
```

### Resource request fails even though resources seem available
Remember that fairshare uses delta-based checking. Check your current allocation:
```bash
pkexec fairshare info
```

Then check system-wide availability:
```bash
pkexec fairshare status
```

Your request fails only if the **net increase** exceeds available resources.

### Changes don't take effect
After running `sudo fairshare admin setup`, systemd needs to reload:
```bash
sudo systemctl daemon-reload
```

## License

This project is open source.
