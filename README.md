# fairshare

A systemd-based resource manager for multi-user Linux systems that provides fair allocation of CPU and memory resources.

**Default allocation per user: 1 CPU core and 2G RAM**

## Overview

`fairshare` uses systemd user slices to manage resource allocation on shared Linux systems. It allows users to request and release resources dynamically while preventing resource over-allocation, and provides administrators with tools to set baseline defaults.

## Features

- View system-wide resource status and per-user allocations
- Request CPU and memory resources for your user session
- Release allocated resources back to defaults
- Check your current resource allocation
- Admin tools for setting global baseline limits

## Installation

Use the provided installation script:

```bash
sudo ./install.sh
```

Or manually:

```bash
cargo build --release
sudo cp target/release/fairshare /usr/local/bin/
```

## Commands

### `status`

Show system totals and all user allocations.

```bash
fairshare status
```

Example output:
```
System total: 16.00 GB RAM / 8 CPUs
Allocated: 12.50 GB RAM / 5.00 CPUs
Available: 3.50 GB RAM / 3.00 CPUs

Per-user allocations:
  UID 1000 � 400.0% CPU, 8.00 GB RAM
  UID 1001 � 100.0% CPU, 4.50 GB RAM
```

### `request`

Request resources for your user session beyond the default 1 CPU and 2G RAM.

```bash
fairshare request --cpu 4 --mem 8
```

Options:
- `--cpu <NUM>`: Number of CPU cores to allocate
- `--mem <NUM>`: Memory in gigabytes to allocate

The command will:
- Check if requested resources are available
- Allocate resources to your systemd user slice
- Fail if resources exceed what's available

### `release`

Release all allocated resources back to system defaults.

```bash
fairshare release
```

This reverts your user slice configuration to the baseline defaults set by the administrator.

### `info`

Show your current user's resource allocation.

```bash
fairshare info
```

Displays the CPUQuota and MemoryMax values for your user slice.

### `admin`

Admin operations (requires root privileges).

#### `admin setup`

Set global baseline resource limits for all users.

```bash
sudo fairshare admin setup --cpu 1 --mem 2
```

Options:
- `--cpu <NUM>`: Number of CPU cores (default: 1)
- `--mem <NUM>`: Memory in gigabytes (default: 2)

This command:
- Creates `/etc/systemd/system/user-.slice.d/00-defaults.conf` with default limits
- Creates `/etc/fairshare/policy.toml` with policy configuration
- Reloads the systemd daemon

## How It Works

`fairshare` interacts with systemd's resource management features:

1. **User Sessions**: Each user has their own systemd user session (accessed via `systemctl --user`)
2. **User Slice**: Users control their own `-.slice` unit which manages their session resources
3. **CPUQuota**: Sets the percentage of CPU time available (100% = 1 core, 400% = 4 cores)
4. **MemoryMax**: Sets the maximum memory the user session can consume
5. **Resource Tracking**: Monitors all user slices to calculate available system resources

When you request resources, `fairshare` uses `systemctl --user set-property` to configure your user session slice. When you release resources, it uses `systemctl --user revert` to restore defaults. No elevated privileges are needed since users manage their own sessions.

## Requirements

- Linux system with systemd (with user session support)
- Rust 1.70+ (for building)
- No sudo access needed for regular users (except `admin setup`)

## Default Resource Allocation

Each user on the system receives by default:
- **1 CPU core** (100% CPU quota)
- **2G RAM** (2000000000 bytes MemoryMax)

Users can request additional resources up to system availability. When resources are released, limits return to these defaults.

## Architecture

The codebase is organized into modules:

- `src/main.rs`: Entry point and command routing (main.rs:1)
- `src/cli.rs`: Command-line interface definitions using clap (cli.rs:1)
- `src/system.rs`: System information gathering and resource calculations (system.rs:1)
- `src/systemd.rs`: Systemd interaction and configuration management (systemd.rs:1)

Key functions:
- `get_system_totals()`: Retrieves total CPU and memory (system.rs:15)
- `get_user_allocations()`: Reads all user slice allocations (system.rs:30)
- `check_request()`: Validates resource requests against availability (system.rs:82)
- `set_user_limits()`: Applies resource limits via systemctl (systemd.rs:7)
- `release_user_limits()`: Reverts user slice to defaults (systemd.rs:24)
- `admin_setup_defaults()`: Configures global baseline limits (systemd.rs:53)

## License

This project is open source.
