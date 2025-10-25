release:
	cargo build --release
	cp target/release/fairshare /usr/local/bin/
