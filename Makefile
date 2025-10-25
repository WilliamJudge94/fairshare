release:
	cargo build --release
	cp target/release/fairshare /usr/local/bin/

install: release
	@echo "Installing fairshare binary..."
	install -m 755 -o root -g root target/release/fairshare /usr/local/bin/fairshare
	@echo "Binary installed to /usr/local/bin/fairshare"
	@echo ""
	@echo "Next steps:"
	@echo "  1. Run 'sudo fairshare admin setup --cpu 1 --mem 2' to install configuration"
	@echo "  2. Users can then request resources with 'pkexec fairshare request --cpu N --mem M'"

uninstall:
	@echo "Removing fairshare configuration and binary..."
	@if [ -f /usr/local/bin/fairshare ]; then \
		fairshare admin uninstall 2>/dev/null || true; \
	fi
	rm -f /usr/local/bin/fairshare
	@echo "Fairshare uninstalled"
