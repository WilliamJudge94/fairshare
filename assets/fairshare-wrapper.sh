#!/bin/bash
# fairshare - Resource manager wrapper
# Handles privilege escalation transparently for user commands

set -e

# Auto-detect binary location (supports both package and local installation)
if [[ -x "/usr/libexec/fairshare-bin" ]]; then
    FAIRSHARE_BIN="/usr/libexec/fairshare-bin"
elif [[ -x "/usr/lib/fairshare/fairshare-bin" ]]; then
    FAIRSHARE_BIN="/usr/lib/fairshare/fairshare-bin"
elif [[ -x "/usr/local/libexec/fairshare-bin" ]]; then
    FAIRSHARE_BIN="/usr/local/libexec/fairshare-bin"
else
    echo "Error: fairshare binary not found" >&2
    echo "Searched paths:" >&2
    echo "  - /usr/libexec/fairshare-bin (package installation)" >&2
    echo "  - /usr/lib/fairshare/fairshare-bin (alternative package)" >&2
    echo "  - /usr/local/libexec/fairshare-bin (local installation)" >&2
    exit 1
fi

# Detect if running admin command
# Admin commands start with "admin" subcommand and require sudo
if [[ "${1:-}" == "admin" ]]; then
    # Admin command - execute directly without pkexec
    # User must have already invoked with sudo
    exec "$FAIRSHARE_BIN" "$@"
fi

# Regular user command - use pkexec for privilege escalation
# pkexec will handle authentication and set PKEXEC_UID
exec pkexec "$FAIRSHARE_BIN" "$@"
