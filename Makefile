.PHONY: build clean

build:
	cargo build --release
	RUSTFLAGS='-C target-feature=+crt-static' cargo build --target x86_64-pc-windows-gnu --release
	mkdir -p dist
	cp target/release/math-strat dist
	cp target/x86_64-pc-windows-gnu/release/math-strat.exe dist

clean:
	rm -rf dist
	cargo clean
