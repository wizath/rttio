FEATURES ?= default

.PHONY: help build test release-macos release-linux release-linux-arm clean

help:
	@printf '%s\n' \
		'Targets:' \
		'  make build              build local release binary' \
		'  make test               run tests with locked dependencies' \
		'  make release-macos      build dist tarball for aarch64 macOS' \
		'  make release-linux      build dist tarball for x86_64 Linux' \
		'  make release-linux-arm  build dist tarball for aarch64 Linux' \
		'  make clean              remove dist artifacts' \
		'' \
		'Options:' \
		'  FEATURES=default        default features' \
		'  FEATURES=serial,control build without default features and enable list' \
		'  FEATURES=none           build without default features'

build:
	cargo build --release --locked --bin rttio

test:
	cargo test --locked

release-macos:
	scripts/build-macos.sh --features $(FEATURES)

release-linux:
	scripts/build-linux.sh --features $(FEATURES)

release-linux-arm:
	scripts/build-linux-arm.sh --features $(FEATURES)

clean:
	rm -rf dist
