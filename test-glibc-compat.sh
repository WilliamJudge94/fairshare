#!/bin/bash
# Test script to verify GLIBC compatibility of built binaries
# This script checks the GLIBC requirements of the fairshare binaries

set -e

echo "Testing GLIBC compatibility of fairshare binaries..."
echo "=================================================="

if [ ! -f "releases/fairshare-x86_64" ]; then
    echo "Error: releases/fairshare-x86_64 not found"
    echo "Please run 'make compile-releases' first"
    exit 1
fi

# Check GLIBC version required by the binary
echo -e "\n1. Checking required GLIBC version for x86_64 binary:"
echo "---------------------------------------------------"
objdump -T releases/fairshare-x86_64 | grep GLIBC | sed 's/.*GLIBC_\([0-9.]*\).*/\1/' | sort -V | uniq | tail -1 | xargs -I {} echo "Maximum GLIBC version required: {}"

# Check the binary file type
echo -e "\n2. Binary file information:"
echo "---------------------------------------------------"
file releases/fairshare-x86_64

# List all GLIBC versions referenced
echo -e "\n3. All GLIBC versions referenced:"
echo "---------------------------------------------------"
objdump -T releases/fairshare-x86_64 | grep GLIBC | sed 's/.*GLIBC_\([0-9.]*\).*/GLIBC_\1/' | sort -V | uniq

echo -e "\n4. Compatibility check:"
echo "---------------------------------------------------"
MAX_GLIBC=$(objdump -T releases/fairshare-x86_64 | grep GLIBC | sed 's/.*GLIBC_\([0-9.]*\).*/\1/' | sort -V | uniq | tail -1)
echo "This binary requires GLIBC $MAX_GLIBC or newer"
echo ""
echo "Compatible with:"
echo "  ✓ RHEL 9+ (GLIBC 2.34)"
echo "  ✓ Debian 11+ (GLIBC 2.31)"
echo "  ✓ Ubuntu 20.04+ (GLIBC 2.31)"
echo "  ✓ Rocky Linux 9+ (GLIBC 2.34)"
echo "  ✓ AlmaLinux 9+ (GLIBC 2.34)"

# Compare with the expected maximum (2.31)
if [ "$(printf '%s\n' "2.31" "$MAX_GLIBC" | sort -V | head -n1)" = "2.31" ] && [ "$MAX_GLIBC" != "2.31" ]; then
    echo -e "\n⚠️  WARNING: Binary requires GLIBC $MAX_GLIBC which is newer than the expected 2.31"
    echo "   This may indicate the binary was not built in the Debian 11 container"
    exit 1
else
    echo -e "\n✅ SUCCESS: Binary is compatible with GLIBC 2.31+ systems"
fi
