# Task 2: CLI Implementation & End-to-End Integration

## Objective
Build the user-facing CLI that communicates with the daemon and provides essential resource management commands.

## Deliverables

### 1. CLI Framework
- Add `clap` dependency with derive features
- Create `src/cli.rs` with command structure:
  ```rust
  #[derive(Parser)]
  enum Command {
      Request { cpu: u32, mem: String },
      Release,
      Status,
      Exec { command: Vec<String> },
  }
  ```
- Create `src/main.rs` as CLI entry point
- Handle both CLI mode and daemon mode (via flag or binary name detection)

### 2. IPC Client
- Implement Unix socket client in `ipc.rs`
- Connect to `/run/fairshare.sock`
- Send JSON requests matching protocol from Task 1
- Parse JSON responses
- Handle connection errors gracefully
- Timeout after 5 seconds

### 3. Core Commands

#### `fairshare request --cpu <N> --mem <SIZE>`
- Parse arguments (cpu as integer, mem as string like "8G")
- Send `RequestResources` message to daemon
- Display success or error message
- Example output:
  ```
  ✓ Request approved — limits updated for fairshare-1001.slice
  CPUQuota: 400% (4 cores)
  MemoryMax: 8G
  ```

#### `fairshare release`
- Send `Release` message to daemon
- Display confirmation
- Example output:
  ```
  ✓ Resources released. Slice fairshare-1001.slice removed.
  ```

#### `fairshare status`
- Send `Status` message to daemon
- Display current allocation
- Query systemd for actual slice properties (optional enhancement)
- Example output:
  ```
  Your slice: fairshare-1001.slice
  Allocation: 4 CPUs / 8GB RAM
  Status: Active
  ```

#### `fairshare exec -- <command>`
- Get current user's UID
- Verify slice exists (via `status` call)
- Execute command using `systemd-run`:
  ```bash
  systemd-run --user --scope --slice=fairshare-{uid}.slice -- <command>
  ```
- Stream stdout/stderr to terminal
- Preserve exit code

### 4. User Experience
- Clear error messages:
  - "Daemon not running. Is fairshared started?"
  - "Permission denied. Are you in the 'fairshare' group?"
  - "Request exceeds maximum allowed (max: 8 CPUs, 32GB)"
- Colorized output (optional, using `colored` crate)
- Help text for all commands (`--help`)

### 5. Integration Testing
- Create integration test in `tests/integration_test.rs`:
  - Start daemon in background
  - Run CLI commands
  - Verify slice creation via systemctl
  - Verify process runs in correct slice
  - Clean up slices
- Test error cases (daemon down, invalid requests)

### 6. Documentation
- Update README.md with:
  - Quick start guide
  - Installation instructions (manual for MVP)
  - Usage examples
  - Requirements (Linux, systemd, cgroups v2)
- Add inline code documentation

## Acceptance Criteria
- [ ] CLI binary compiles (`cargo build --release`)
- [ ] `fairshare request` creates a slice with correct limits
- [ ] `fairshare status` shows current allocation
- [ ] `fairshare exec` runs commands in the user's slice
- [ ] `fairshare release` removes the slice
- [ ] Error messages are clear and actionable
- [ ] `--help` works for all commands
- [ ] Integration test passes end-to-end workflow
- [ ] README documents installation and basic usage

## End-to-End Test Scenario
```bash
# Terminal 1: Start daemon
sudo cargo run --bin fairshared

# Terminal 2: User workflow
fairshare status
# Expected: "No active allocation"

fairshare request --cpu 4 --mem 8G
# Expected: Success message

fairshare status
# Expected: Shows 4 CPUs / 8GB

systemctl status fairshare-$(id -u).slice
# Expected: Active slice with correct properties

fairshare exec -- stress-ng --cpu 4 --timeout 10s
# Expected: Process runs under slice, limited to 4 CPUs

fairshare release
# Expected: Slice removed

systemctl status fairshare-$(id -u).slice
# Expected: Slice not found
```

## Out of Scope (for MVP)
- Admin commands (`admin list`, `admin set-max`, etc.)
- Web dashboard
- APT packaging
- Systemd service installation
- Multi-user status (global view)
- GPU enforcement
- Dynamic rebalancing
- Cluster integration

## Dependencies
- Requires Task 1 (daemon) to be complete
- Both tasks together form a working MVP
