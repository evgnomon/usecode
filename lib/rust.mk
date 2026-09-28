# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Shared rules for the Rust crates in the workspace (see /Cargo.toml).
# A crate's Makefile sets NAME (its package and binary name), optionally
# EXTRA_SOURCES and PREFIX, and then includes this file.

ROOT_DIR ?= $(abspath $(dir $(lastword $(MAKEFILE_LIST)))..)
SOURCES := $(shell find src -type f) Cargo.toml $(ROOT_DIR)/Cargo.lock $(EXTRA_SOURCES)
PROFILE ?= release
OUTPUT := $(ROOT_DIR)/target/$(PROFILE)/$(NAME)
DESTDIR ?= /
PREFIX ?= /usr/local
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/$(NAME)
LINK_BIN := $(HOME)/.local/bin/$(NAME)

.PHONY: all build check install uninstall link unlink test lint fmt clean

all: build

build: $(OUTPUT)

$(OUTPUT): $(SOURCES)
	@cargo build -p $(NAME) --profile $(PROFILE)
	@touch $(OUTPUT)

install: $(INSTALL_BIN)

$(INSTALL_BIN): $(OUTPUT)
	@install -d $(@D)
	@install -m 0755 $(OUTPUT) $@

link: $(OUTPUT)
	@mkdir -p $(dir $(LINK_BIN))
	@ln -sf $(OUTPUT) $(LINK_BIN)

unlink:
	@rm -f $(LINK_BIN)

check:
	@cargo fmt -p $(NAME) --check
	@cargo clippy -p $(NAME) --all-targets -- -D warnings
	@cargo test -p $(NAME)

test:
	@cargo test -p $(NAME)

lint:
	@cargo clippy -p $(NAME) --all-targets -- -D warnings

fmt:
	@cargo fmt -p $(NAME)

uninstall:
	@rm -f $(INSTALL_BIN)

clean:
	@cargo clean -p $(NAME)
