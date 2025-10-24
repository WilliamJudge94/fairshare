#!/bin/bash
#
# Test script for systemd_client.rs implementation
#
# This script should be run after Rust/Cargo is installed
# and on a system with systemd available
#

set -e  # Exit on error

echo "========================================"
echo "Fairshare Systemd Client Test Suite"
echo "========================================"
echo ""

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    echo "ERROR: cargo not found. Please install Rust:"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check if systemd is available
if ! command -v systemctl &> /dev/null; then
    echo "WARNING: systemctl not found. Integration tests will be skipped."
    SKIP_INTEGRATION=1
else
    echo "✓ systemctl found"
    SKIP_INTEGRATION=0
fi

# Check if we have root privileges
if [ "$EUID" -ne 0 ]; then
    echo "WARNING: Not running as root. Integration tests may fail."
    echo "         Run with sudo to enable full testing."
    echo ""
fi

echo "========================================"
echo "Step 1: Build the project"
echo "========================================"
cargo build
echo "✓ Build successful"
echo ""

echo "========================================"
echo "Step 2: Run unit tests"
echo "========================================"
cargo test --lib -- --nocapture
echo "✓ Unit tests passed"
echo ""

if [ $SKIP_INTEGRATION -eq 0 ]; then
    echo "========================================"
    echo "Step 3: Run integration tests"
    echo "========================================"
    echo "NOTE: These tests require systemd and may require root privileges"
    echo ""

    if [ "$EUID" -eq 0 ]; then
        cargo test --lib -- --ignored --nocapture
        echo "✓ Integration tests completed"
    else
        echo "Skipping integration tests (requires root)"
        echo "Run with sudo to execute integration tests:"
        echo "  sudo -E cargo test --lib -- --ignored --nocapture"
    fi
    echo ""
fi

echo "========================================"
echo "Step 4: Check for warnings"
echo "========================================"
cargo clippy --all-targets -- -D warnings
echo "✓ No warnings found"
echo ""

echo "========================================"
echo "Step 5: Format check"
echo "========================================"
cargo fmt -- --check
echo "✓ Code is properly formatted"
echo ""

echo "========================================"
echo "All checks completed successfully!"
echo "========================================"
echo ""
echo "Next steps:"
echo "  1. Review SYSTEMD_CLIENT_IMPLEMENTATION.md for details"
echo "  2. Test manually with busctl (see documentation)"
echo "  3. Integrate with daemon.rs"
echo ""
