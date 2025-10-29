.PHONY: help release build-release compile-releases compile-x86_64 clean setup-cross

# Show available targets
help:
	@echo "fairshare Makefile"
	@echo ""
	@echo "Setup (one-time):"
	@echo "  make setup-cross    - Install tools for cross-compilation"
	@echo ""
	@echo "Build:"
	@echo "  make release        - Build and install to /usr/local/ (requires sudo)"
	@echo "  make compile-x86_64 - Build x86_64 release binary"
	@echo "  make compile-releases - Build x86_64 and aarch64 binaries"
	@echo "  make clean          - Clean release artifacts"

# Install and configure cross-compilation tools
setup-cross:
	@echo "Setting up cross-compilation environment..."
	rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
	apt-get update && apt-get install -y gcc-aarch64-linux-gnu
	mkdir -p .cargo
	echo "[target.aarch64-unknown-linux-gnu]" > .cargo/config.toml
	echo "linker = \"aarch64-linux-gnu-gcc\"" >> .cargo/config.toml
	@echo ""
	@echo "✓ Setup complete. Run 'make compile-releases' to build binaries."

# Build and install to system
release:
	cargo build --release
	install -D -m 0755 target/release/fairshare /usr/local/libexec/fairshare-bin
	install -D -m 0755 assets/fairshare-wrapper.sh /usr/local/bin/fairshare

# Build x86_64 release binary
compile-x86_64: clean
	@echo "Building x86_64 binary..."
	mkdir -p releases
	cargo build --release --target x86_64-unknown-linux-gnu
	cp target/x86_64-unknown-linux-gnu/release/fairshare releases/fairshare-x86_64
	strip releases/fairshare-x86_64
	cd releases && sha256sum fairshare-x86_64 > SHA256SUMS
	@echo "✓ Built: releases/fairshare-x86_64"

# Build both x86_64 and aarch64 release binaries
compile-releases: clean
	@echo "Building release binaries..."
	mkdir -p releases

	# x86_64
	cargo build --release --target x86_64-unknown-linux-gnu
	cp target/x86_64-unknown-linux-gnu/release/fairshare releases/fairshare-x86_64
	strip releases/fairshare-x86_64

	# aarch64
	cargo build --release --target aarch64-unknown-linux-gnu
	cp target/aarch64-unknown-linux-gnu/release/fairshare releases/fairshare-aarch64
	aarch64-linux-gnu-strip releases/fairshare-aarch64 2>/dev/null || true

	# Generate checksums
	cd releases && sha256sum fairshare-* > SHA256SUMS
	@echo ""
	@echo "✓ Built: releases/fairshare-x86_64 and releases/fairshare-aarch64"
	@ls -lh releases/

# Clean release artifacts
clean:
	@rm -rf releases/fairshare-* releases/SHA256SUMS
