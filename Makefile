.PHONY: build run release test lint fmt fmt-check check clean

build:
	cargo build

release:
	cargo build --release

run:
	cargo run --release

test:
	cargo test

lint:
	cargo clippy --all-targets -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

check: fmt-check lint test

clean:
	cargo clean
