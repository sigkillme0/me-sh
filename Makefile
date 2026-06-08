.PHONY: check fmt lint test build smoke package publish-dry-run clean

check: fmt lint test build smoke

fmt:
	cargo fmt --check

lint:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

build:
	cargo build --release

smoke: build
	./target/release/mesh --version
	./target/release/mesh --help
	MESH_CONFIG=/tmp/mesh-empty-config.json ./target/release/mesh status

package:
	cargo package

publish-dry-run:
	cargo publish --dry-run

clean:
	cargo clean
