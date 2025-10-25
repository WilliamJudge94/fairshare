# fairshare

A resource manager for shared Linux systems that prevents users from monopolizing CPU and memory.

## Why You Need This

On multi-user Linux systems, one user can consume all CPU and memory, leaving nothing for others. **fairshare** automatically limits what each user can use while allowing them to request more when needed—fairly allocating resources across all users.

**Default allocation per user: 1 CPU core and 2GB RAM**

## Installation

### Using Make
```bash
make release
```

### Manual Build
```bash
cargo build --release
sudo cp target/release/fairshare /usr/local/bin/
```

## How to Use

### Check system resources
```bash
fairshare status
```
Shows how much CPU and memory is available and how much each user is using.

### Request more resources
```bash
fairshare request --cpu 4 --mem 8
```
Ask for 4 CPU cores and 8GB RAM. The system will grant your request only if resources are available.
- CPU: 1-1000 cores
- Memory: 1-10000 GB

### Check your allocation
```bash
fairshare info
```
See how much CPU and memory your user session currently has.

### Release resources
```bash
fairshare release
```
Give back your resources to return to the default 1 CPU core and 2GB RAM.

### Admin: Set default limits (requires root)
```bash
sudo fairshare admin setup --cpu 1 --mem 2
```
Set what every user gets by default when they first log in.

### Admin: Remove fairshare (requires root)
```bash
sudo fairshare admin uninstall --force
```
Uninstall fairshare and revert to system defaults.

## Requirements

- Linux system with systemd (with user session support)
- Rust 1.70+ (for building only)

## License

This project is open source.
