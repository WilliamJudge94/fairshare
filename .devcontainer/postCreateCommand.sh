#!/bin/bash

# Set up workspace directory
WORKSPACE_DIR="/workspaces/$(basename "$PWD")"
if [ -d "$WORKSPACE_DIR" ]; then
    cd "$WORKSPACE_DIR"
else
    echo "Workspace directory not found, using current directory: $PWD"
fi

#!/usr/bin/env bash
set -euo pipefail

# Determine PID 1 without requiring 'ps'
pid1_comm="$(cat /proc/1/comm 2>/dev/null || echo unknown)"

if [[ "$pid1_comm" != "systemd" ]]; then
  echo "WARNING: PID 1 is '$pid1_comm', not 'systemd'."
  echo "Hints:"
  echo "  - Ensure devcontainer.json has: \"overrideCommand\": false"
  echo "  - Ensure runArgs includes: --privileged, --cgroupns=host, and /sys/fs/cgroup mount"
  echo "  - Ensure you're USING the Dockerfile build (not 'image') and you REBUILT the container"
  echo "  - After changing config: Command Palette → Dev Containers: Rebuild Container"
  # Do NOT exit; allow environment setup to proceed
fi

# Install headers/libs you need (idempotent)
apt-get update
apt-get install -y --no-install-recommends \
  libsystemd-dev libdbus-1-dev dbus git pkg-config build-essential \
  && rm -rf /var/lib/apt/lists/*

# Optional: enable user lingering so 'systemctl --user' works without PAM session
if command -v loginctl >/dev/null 2>&1; then
  loginctl enable-linger "${USER:-root}" || true
fi

# Light diagnostics
systemctl --version || true
systemctl is-system-running || true

echo "postCreate completed. PID1='$pid1_comm'. If not 'systemd', rebuild with the provided config."
