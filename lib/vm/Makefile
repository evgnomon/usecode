# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

SOURCES := $(shell find src -type f \( -name "*.rs" -o -name "*.c" -o -name "*.h" \)) Cargo.toml Cargo.lock build.rs
PROFILE ?= release
OUTPUT := target/$(PROFILE)/vm
DESTDIR ?= /
PREFIX ?= /usr
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/vm

.PHONY: all build check install uninstall test lint fmt dist publish clean

all: build

build: $(OUTPUT)

$(OUTPUT): $(SOURCES)
	@cargo build --profile $(PROFILE)
	@touch $(OUTPUT)

install: $(INSTALL_BIN)

$(INSTALL_BIN): $(OUTPUT)
	@install -d $(@D)
	@install -m 0755 $(OUTPUT) $@

check:
	@cargo fmt --check
	@cargo clippy --all-targets -- -D warnings
	@cargo test

test:
	@cargo test

lint:
	@cargo clippy --all-targets -- -D warnings

fmt:
	@cargo fmt

dist: build
	@mkdeb

publish: dist
	@pubdeb

uninstall:
	@rm -f $(INSTALL_BIN)

clean:
	@cargo clean
