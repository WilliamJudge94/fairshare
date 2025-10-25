release:
	cargo build --release
	install -D -m 0755 target/release/fairshare /usr/local/libexec/fairshare-bin
	install -D -m 0755 assets/fairshare-wrapper.sh /usr/local/bin/fairshare
