SHELL := /bin/sh

PACKAGE := wrk
VERSION ?= $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
TARGET ?= $(shell rustc -vV | sed -n 's/^host: //p')
DIST_DIR := dist
PYTHON ?= python3
CARGO_BUILD_ARGS := --locked --release --target $(TARGET)

.PHONY: help fmt fmt-check clippy test check build package package-check clean package-linux package-macos package-windows

help:
	@printf '%s\n' \
		'make fmt              Run rustfmt' \
		'make fmt-check        Check formatting without rewriting files' \
		'make clippy           Run clippy with warnings denied' \
		'make test             Run the full test suite' \
		'make check            Run fmt-check, clippy, and tests' \
		'make build            Build a release binary for TARGET=$(TARGET)' \
		'make package          Build and archive a release binary for TARGET=$(TARGET)' \
		'make package-check    Run cargo package dry-run verification' \
		'make package-linux    Build a Linux archive (requires target toolchain)' \
		'make package-macos    Build a macOS archive (requires target toolchain)' \
		'make package-windows  Build a Windows archive (requires target toolchain)' \
		'make clean            Remove build artifacts'

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --all-targets --all-features

check: fmt-check clippy test

build:
	cargo build $(CARGO_BUILD_ARGS)

package: build
	$(PYTHON) scripts/package_release.py --target $(TARGET) --version $(VERSION) --name $(PACKAGE) --dist-dir $(DIST_DIR)

package-check:
	cargo package --locked --allow-dirty

package-linux:
	$(MAKE) package TARGET=x86_64-unknown-linux-gnu

package-macos:
	$(MAKE) package TARGET=aarch64-apple-darwin

package-windows:
	$(MAKE) package TARGET=x86_64-pc-windows-msvc

clean:
	cargo clean
	rm -rf $(DIST_DIR)
