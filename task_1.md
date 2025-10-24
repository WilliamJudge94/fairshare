# Task 1: Core Daemon & Systemd Integration

## Objective
Build the fairshared daemon that can create and manage systemd slices with CPU and memory limits via DBus.

## Deliverables

### 1. Project Setup
- Initialize Rust project with Cargo.toml
- Add dependencies: `tokio`, `zbus`, `serde`, `serde_yaml`, `tracing`, `tracing-subscriber`
- Create basic module structure:
  - `src/daemon.rs` - main daemon loop
  - `src/policy.rs` - YAML policy parsing
  - `src/systemd_client.rs` - DBus interface to systemd
  - `src/ipc.rs` - Unix socket protocol
  - `src/utils.rs` - shared utilities

### 2. Policy System
- Implement YAML policy parser for `/etc/fairshare/policy.d/default.yaml`
- Support structure:
  ```yaml
  defaults:
    cpu: 2
    mem: 8G
  max:
    cpu: 8
    mem: 32G
  ```
- Validate limits (max >= defaults, positive values)
- Parse memory units (G, GB, M, MB)

### 3. Systemd DBus Client
- Implement `systemd_client.rs` with zbus
- Functions:
  - `create_slice(uid: u32, cpu: u32, mem: String) -> Result<()>`
  - `remove_slice(uid: u32) -> Result<()>`
  - `get_slice_status(uid: u32) -> Result<SliceInfo>`
- Use `StartTransientUnit` DBus method
- Set properties: `CPUQuota`, `MemoryMax`, `TasksMax`
- Handle slice naming: `fairshare-{uid}.slice`

### 4. IPC Protocol
- Define JSON message protocol:
  ```rust
  enum Request {
      RequestResources { cpu: u32, mem: String },
      Release,
      Status,
  }

  enum Response {
      Success { message: String },
      Error { error: String },
      StatusInfo { allocated_cpu: u32, allocated_mem: String },
  }
  ```
- Implement Unix socket server at `/run/fairshare.sock`
- Handle multiple concurrent connections (tokio async)
- Authenticate requests by socket peer credentials (UID)

### 5. Daemon Main Loop
- Accept connections on Unix socket
- Parse incoming requests
- Validate against policy (enforce max limits)
- Call systemd_client to create/remove slices
- Track active allocations in-memory (HashMap<UID, Allocation>)
- Respond to client with success/error
- Structured logging with tracing

### 6. Basic Error Handling
- Graceful error messages for:
  - DBus connection failures
  - Invalid resource requests (exceeding max)
  - Systemd API errors
  - Socket permission issues

## Acceptance Criteria
- [ ] Daemon starts and creates Unix socket at `/run/fairshare.sock`
- [ ] Daemon reads and validates policy from YAML file
- [ ] Can create a systemd slice with CPU and memory limits via DBus
- [ ] Can remove 1a systemd slice
- [ ] Handles concurrent requests from multiple users
- [ ] EnfoIcy
- [ ] Logs all operations to stdout (tracing)
- [ ] Compiles and runs on Linux (tested manually)

## Testing Approach
- Manual testing with:
  ```bash
  # Test DBus integration
  busctl call org.freedesktop.systemd1 \
    /org/freedesktop/systemd1 \
    org.freedesktop.systemd1.Manager \
    GetUnit s "fairshare-1001.slice"

  # Verify slice properties
  systemctl show fairshare-1001.slice
  ```
- Test socket communication with netcat/socat
- Verify policy enforcement with various request combinations

## Out of Scope (for MVP)
- CLI implementation (covered in Task 2)
- Systemd service unit
- Packaging (.deb)
- Admin commands
- Per-user policies (only default.yaml)
- SELinux/AppArmor profiles
