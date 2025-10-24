# Fairshare Design Document

## 1. Overview

**fairshare** is a system utility for resource fairness management on multi-user Linux systems.

It allows:
- Normal users to query and request CPU / RAM allocations
- Administrators to define policies (defaults, max caps)
- System enforcement through systemd slices and cgroups v2

It consists of:
- **fairshared daemon** — root-level service managing slices via DBus
- **fairshare CLI** — user tool to view, request, and release resources

## 2. Motivation

Shared Linux hosts (labs, HPC nodes, dev servers) often crash when users over-consume compute.

Systemd and cgroups can enforce limits, but configuration is opaque and static.

**fairshare** automates this process:
- Creates dynamic per-user slices (`user-<UID>.slice/fairshare-<UID>.slice`)
- Enforces quotas per user
- Exposes a simple CLI for visibility and self-service

## 3. System Architecture

### 3.1 Components

| Component | Description |
|-----------|-------------|
| **fairshare (CLI)** | Executed by users to request resources, query status, or run commands inside allocated slices |
| **fairshared (daemon)** | Runs as root; communicates with systemd over DBus; enforces allocations and policies |
| **Policy Store** | YAML files in `/etc/fairshare/policy.d/`; define defaults, max limits, and admin group |
| **Unix Socket** | `/run/fairshare.sock` for local IPC between CLI ↔ daemon |
| **Systemd** | Manages per-user slices that implement CPU & memory limits |

### 3.2 Architecture Diagram

```
          ┌───────────────┐
          │   User CLI    │  fairshare request --cpu 4 --mem 8G
          └──────┬────────┘
                 │  Unix Socket (/run/fairshare.sock)
                 ▼
          ┌───────────────┐
          │  fairshared   │ (root daemon)
          │  (Rust)       │
          ├───────────────┤
          │ Validate req  │
          │ Check policy  │
          │ Apply limits  │
          └──────┬────────┘
                 │
                 ▼
          ┌───────────────┐
          │  systemd/DBus │
          │  Create slice │
          └───────────────┘
```

## 4. Rust Implementation Plan

### 4.1 Language & Libraries

| Area | Crate | Purpose |
|------|-------|---------|
| CLI | `clap` | Argument parsing (supports subcommands & auto-help) |
| Config | `serde_yaml`, `serde` | Read/write policy files |
| IPC | `tokio`, `tokio::net::UnixStream`, `serde_json` | Async socket comms |
| Systemd | `zbus`, `systemd` | DBus API for slice creation (`Manager::StartTransientUnit`) |
| Logging | `tracing`, `tracing-subscriber` | Structured logging |
| Daemonization | `systemd` crate | Integrate with journal and notify systemd |
| Packaging | `cargo-deb` | Build .deb packages |

### 4.2 Module Layout

```
fairshare/
 ├── Cargo.toml
 ├── src/
 │   ├── main.rs                ← CLI entry
 │   ├── cli.rs                 ← CLI command definitions
 │   ├── ipc.rs                 ← Unix socket protocol
 │   ├── policy.rs              ← YAML parsing & validation
 │   ├── systemd_client.rs      ← DBus interface to systemd
 │   ├── daemon.rs              ← fairshared main loop
 │   └── utils.rs
 ├── packaging/
 │   ├── systemd/fairshared.service
 │   ├── default.yaml
 │   └── Makefile
 └── docs/
     └── design/fairshare.md
```

## 5. Command Interface

### 5.1 User Commands

| Command | Description |
|---------|-------------|
| `fairshare status` | Show system and user resource usage |
| `fairshare request --cpu 4 --mem 8G` | Request a slice with given limits |
| `fairshare release` | Release user's slice |
| `fairshare exec -- <cmd>` | Run a process inside user's slice |
| `fairshare whoami` | Show your UID and slice association |

### 5.2 Admin Commands

| Command | Description |
|---------|-------------|
| `fairshare admin list` | List all users' current allocations |
| `fairshare admin set-default --cpu 2 --mem 4G` | Set default policy |
| `fairshare admin set-max --cpu 8 --mem 32G` | Set global cap |
| `fairshare admin revoke <user>` | Revoke user's allocation |
| `fairshare admin reload` | Reload policy files |

## 6. Systemd Slice Model

Each user gets a dedicated sub-slice under their user slice:

```
user.slice/
 └── user-1001.slice/
      └── fairshare-1001.slice/
```

Applied properties:

```ini
[Slice]
CPUQuota=400%
MemoryMax=8G
TasksMax=1024
```

The daemon communicates with systemd over DBus:

```rust
let manager = zbus::ProxyBuilder::new(&connection)
    .destination("org.freedesktop.systemd1")?
    .interface("org.freedesktop.systemd1.Manager")?
    .path("/org/freedesktop/systemd1")?
    .build()
    .await?;

manager.call("StartTransientUnit", &(
    format!("fairshare-{}.slice", uid),
    "replace",
    props,  // key=value pairs (CPUQuota, MemoryMax)
    vec![],
)).await?;
```

## 7. Configuration

### 7.1 Default Policy

`/etc/fairshare/policy.d/default.yaml`:

```yaml
defaults:
  cpu: 2
  mem: 8G
  tasks: 1024

max:
  cpu: 8
  mem: 32G

admin_group: sysadmins
```

### 7.2 Per-user Policy (optional)

`/etc/fairshare/policy.d/<username>.yaml`:

```yaml
user: alice
default:
  cpu: 4
  mem: 12G
```

## 8. Security Model

| Layer | Control |
|-------|---------|
| Privilege boundary | CLI runs unprivileged; daemon runs as root |
| IPC | Unix socket `/run/fairshare.sock` with `0660` permissions, owned by `root:fairshare`. Users must belong to group `fairshare` to request resources |
| Policy validation | Daemon enforces all limits, rejecting any request exceeding configured max |
| Logging | All actions journaled (`MESSAGE_ID=FAIRSHARE_EVENT`) |
| Audit | Each allocation logged with UID, timestamp, and limits |
| SELinux/AppArmor | Optional profiles provided in `packaging/selinux/` |

## 9. Packaging and Distribution

### 9.1 Build & Install

```bash
cargo build --release
cargo deb
sudo dpkg -i target/debian/fairshare_1.0.0_amd64.deb
```

### 9.2 Package Contents

| Path | Content |
|------|---------|
| `/usr/bin/fairshare` | CLI binary |
| `/usr/sbin/fairshared` | Daemon binary |
| `/etc/systemd/system/fairshared.service` | Unit file |
| `/etc/fairshare/policy.d/default.yaml` | Default policy |
| `/run/fairshare.sock` | IPC socket (created at runtime) |

### 9.3 Example systemd unit

`packaging/systemd/fairshared.service`:

```ini
[Unit]
Description=Fairshare Resource Manager Daemon
After=network.target

[Service]
ExecStart=/usr/sbin/fairshared
Restart=on-failure
NotifyAccess=all

[Install]
WantedBy=multi-user.target
```

## 10. Installation via APT

1. Build .deb via `cargo-deb`
2. Host via Cloudsmith or GitHub Pages as an APT repo
3. Users install:

```bash
curl -fsSL https://yourname.github.io/fairshare/gpg.key | sudo tee /usr/share/keyrings/fairshare.gpg
echo "deb [signed-by=/usr/share/keyrings/fairshare.gpg] https://yourname.github.io/fairshare stable main" | sudo tee /etc/apt/sources.list.d/fairshare.list
sudo apt update
sudo apt install fairshare
```

## 11. Future Extensions

| Feature | Description |
|---------|-------------|
| GPU enforcement | Integrate nvidia-ml or amdgpu device cgroup control |
| Web dashboard / REST API | Lightweight HTTP interface for usage monitoring |
| Dynamic rebalancing | Idle resources can be temporarily redistributed |
| Cluster integration | Hook into Slurm or Kubernetes node agents |

## 12. Example Usage

```bash
$ fairshare status
System: 64 CPUs / 256GB RAM
Available: 52 CPUs / 190GB
Your slice: fairshare-1001.slice
Allocation: 4 CPUs / 12GB

$ fairshare request --cpu 8 --mem 16G
Request approved — limits updated for fairshare-1001.slice

$ fairshare exec -- jupyter lab
Running under fairshare-1001.slice (CPUQuota=800%, MemoryMax=16G)

$ fairshare release
Resources released. Slice removed.
```

## 13. Milestones

| Phase | Deliverable | Est. |
|-------|------------|------|
| v0.1 | CLI skeleton + static policy enforcement | 1 week |
| v0.2 | DBus integration with systemd | 2 weeks |
| v0.3 | Daemon + socket IPC | 3 weeks |
| v0.4 | Policy reload + admin commands | 4 weeks |
| v1.0 | Full packaging + Cloudsmith apt repo | 5 weeks |

## 14. Risks and Mitigation

| Risk | Mitigation |
|------|------------|
| DBus access errors | Graceful retry, detailed error mapping |
| Slice naming collision | Prefix with "fairshare-<uid>" |
| User bypass | Monitor processes outside slice; optional audit cron |
| Misconfigurations | Schema validation for YAML policies |
| Systemd version drift | Use zbus introspection to adapt to runtime API |

## 15. Conclusion

**fairshare** brings system-level fairness to shared Linux environments with minimal administrative overhead.

Built in Rust for safety and performance, it uses systemd primitives to manage compute quotas transparently.

It's self-contained, auditable, and easily distributable as a signed .deb package installable via apt.