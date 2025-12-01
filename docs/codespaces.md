# Developing fairshare in GitHub Codespaces
Fairshare relies on systemd, DBus, and cgroup v2 to manage CPU and memory allocations via systemd slice properties. 
Many local Linux environments (especially GNOME / Ubuntu Desktop) have unstable user session DBus instances,
polkit dialogs, AppArmor restrictions, or mixed cgroup setups that cause unexpected failures when testing fairshare.

GitHub Codespaces provides a clean, fully reproducible, systemd-based environment that matches fairshare’s expectations and allows 
all tests to pass successfully without any special privileges or system configuration.

This document describes how to develop, run, debug, and test fairshare inside a Codespaces environment.

---

## Why Codespaces Works Well for fairshare

Codespaces provides:

- **systemd PID 1**
- **systemd-logind**
- **stable `/run/user/0` user session**
- **full cgroup v2 support**
- **real DBus (system + user)**  
- **root user session (no polkit prompts)**
- **clean environment (no GNOME, no portals, no FUSE/gvfs issues)**

This gives fairshare exactly what it needs for reliable development:

- slice configuration under `/etc/systemd/system.control`
- CPUQuota and MemoryMax changes applied immediately
- consistent DBus communication with systemd
- correct cgroup propagation
- predictable test behavior

---

## Environment Setup

Open a Codespace for the repository.  
Then run the following in every new terminal:

```bash
export XDG_RUNTIME_DIR=/run/user/0
export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus
```

Verify systemd is running:

```bash
systemctl status
```

You should see: 

```bash
/lib/systemd/systemd --user
(sd-pam)
user.slice/user-0.slice/user@0.service
```

Running fairshare inside Codespaces:

Check system totals:

```bash
cargo run -- status
```

Request CPU + memory:

```bash
cargo run -- request --cpu 1 --mem 1
```

Example output:

```bash
✓ Allocated 1 CPU(s) and 1G RAM.
```

Show user allocation:
```bash
cargo run -- info
```

Release limits:
```bash
cargo run -- release
```
This removes slice override files:
```bash
/etc/systemd/system.control/user-0.slice.d/50-CPUQuota.conf
/etc/systemd/system.control/user-0.slice.d/50-MemoryMax.conf
```

Running the Complete Test Suite

Codespaces is capable of running all fairshare tests, including:
- unit tests
- CLI tests
- integration tests

Run:
```bash
cargo test -- --nocapture
```
Output should show 100% passing tests:
```bash
57 passed (unit tests)
32 passed (CLI tests)
5 passed (integration tests)
0 failed
```

## Inspecting systemd slices

To inspect the user slice:

```bash
systemctl status user-0.slice
```

Inspect override drop-in files:

```bash
ls /etc/systemd/system.control/user-0.slice.d/
```

Check CPU quota as applied by systemd:

```bash
systemctl show user-0.slice | grep CPUQuota
```

Check memory limit:

```bash
systemctl show user-0.slice | grep MemoryMax
```

## Troubleshooting

`systemctl --user` status fails

Codespaces sometimes requires the two environment variables to be exported:

export XDG_RUNTIME_DIR=/run/user/0
export DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus

Avoid running `/lib/systemd/systemd --user` manually

Running systemd --user directly (especially in the foreground) and pressing
CTRL+C will terminate your user systemd instance.

Codespaces will not automatically restart it.

If this happens, simply open a new terminal tab.

Desktop Linux behaves differently
Local machines may have:
- polkit authentication dialogs
- GNOME session DBus interference
- gvfs/fuse mounts failing
- AppArmor rules blocking systemd slice writes
- non-root users requiring explicit privileges
- mixed cgroup v1/v2 configurations
Codespaces avoids all of these issues.

## CI/CD or automation

Since Codespaces provides a fully working systemd environment, it can also serve
as a testing baseline for automated CI that requires systemd.

## Summary

GitHub Codespaces is currently the most consistent and reliable environment for
developing and testing fairshare:
- All systemd functionality works
- All DBus calls succeed
- All tests pass (94/94)
- CPU and memory slice controls apply correctly
- No host system modifications required
- No polkit or sudo needed

This document provides a clear path for contributors to work with fairshare in a clean, reproducible, developer-friendly environment.

Reference codespace log: [LOG](codespaces_log.txt)


