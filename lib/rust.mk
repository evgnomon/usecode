# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Shared rules for the Rust crates in the workspace (see /Cargo.toml).
# A crate's Makefile sets NAME (its package name), BIN (its binary name,
# uc-<group>-<command> after the uc command that runs it), optionally
# EXTRA_SOURCES, PREFIX and ALIASES (other names install and link add as
# symlinks to BIN), and then includes this file.

ROOT_DIR ?= $(abspath $(dir $(lastword $(MAKEFILE_LIST)))..)
SOURCES := $(shell find src -type f) Cargo.toml $(ROOT_DIR)/Cargo.lock $(EXTRA_SOURCES)
BIN ?= $(NAME)
PROFILE ?= release
OUTPUT := $(ROOT_DIR)/target/$(PROFILE)/$(BIN)
DESTDIR ?= /
PREFIX ?= /usr/local
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/$(BIN)
LINK_BIN := $(HOME)/.local/bin/$(BIN)
ALIASES ?=
INSTALL_ALIASES := $(addprefix $(dir $(INSTALL_BIN)),$(ALIASES))
LINK_ALIASES := $(addprefix $(dir $(LINK_BIN)),$(ALIASES))

.PHONY: all build check install uninstall link unlink test lint fmt clean

all: build

build: $(OUTPUT)

$(OUTPUT): $(SOURCES)
	@cargo build -p $(NAME) --profile $(PROFILE)
	@touch $(OUTPUT)

install: $(INSTALL_BIN) $(INSTALL_ALIASES)

$(INSTALL_BIN): $(OUTPUT)
	@install -d $(@D)
	@install -m 0755 $(OUTPUT) $@

$(INSTALL_ALIASES): $(INSTALL_BIN)
	@ln -sf $(BIN) $@

link: $(OUTPUT)
	@mkdir -p $(dir $(LINK_BIN))
	@ln -sf $(OUTPUT) $(LINK_BIN)
	@$(foreach alias,$(LINK_ALIASES),ln -sf $(BIN) $(alias);)

unlink:
	@rm -f $(LINK_BIN) $(LINK_ALIASES)

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
	@rm -f $(INSTALL_BIN) $(INSTALL_ALIASES)

clean:
	@cargo clean -p $(NAME)
