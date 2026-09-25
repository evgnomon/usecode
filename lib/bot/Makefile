# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

SOURCES := $(shell find src -type f -name "*.rs") Cargo.toml Cargo.lock
PROFILE ?= release
OUTPUT := target/$(PROFILE)/usecode-mcp
DESTDIR ?= /
PREFIX ?= /usr/local
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/usecode-mcp

.PHONY: all build install uninstall test lint fmt clean

all: build

build: $(OUTPUT)

$(OUTPUT): $(SOURCES)
	@cargo build --profile $(PROFILE)
	@touch $(OUTPUT)

install: $(INSTALL_BIN)

$(INSTALL_BIN): $(OUTPUT)
	@install -d $(@D)
	@install -m 0755 $(OUTPUT) $@

test:
	@cargo test

lint:
	@cargo clippy --all-targets -- -D warnings

fmt:
	@cargo fmt

uninstall:
	@rm -f $(INSTALL_BIN)

clean:
	@cargo clean
