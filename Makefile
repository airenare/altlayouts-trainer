CARGO ?= cargo
TARGET ?= $(shell rustc -vV 2>/dev/null | sed -n 's/^host: //p')
BUILD_ARGS := $(if $(TARGET),--target $(TARGET),)

.PHONY: help build release run test fmt check clean

help:
	@printf '%s\n' 'make build    Build a debug executable' \
	  'make release  Build an optimized executable' \
	  'make run      Run the trainer' \
	  'make test     Run unit tests' \
	  'make fmt      Format Rust source' \
	  'make check    Check formatting and run tests' \
	  'make clean    Remove Cargo build output'

build:
	$(CARGO) build $(BUILD_ARGS)

release:
	$(CARGO) build --release $(BUILD_ARGS)

run:
	$(CARGO) run --release $(BUILD_ARGS)

test:
	$(CARGO) test $(BUILD_ARGS)

fmt:
	$(CARGO) fmt

check:
	$(CARGO) fmt --check
	$(CARGO) test $(BUILD_ARGS)

clean:
	$(CARGO) clean
