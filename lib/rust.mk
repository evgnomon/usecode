# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Shared rules for the Rust crates in the workspace (see /Cargo.toml).
# A crate's Makefile sets NAME (its package name), BIN (its binary name,
# uc-<group>-<command> after the uc command that runs it), optionally
# EXTRA_BINS (other binaries the same package builds and installs - e.g.
# the daemon's control-node tools), EXTRA_SOURCES, PREFIX and ALIASES
# (other names install and link add as symlinks to BIN), and then
# includes this file.

ROOT_DIR ?= $(abspath $(dir $(lastword $(MAKEFILE_LIST)))..)
include $(ROOT_DIR)/lib/profile.mk
SOURCES := $(shell find src -type f) Cargo.toml $(ROOT_DIR)/Cargo.lock $(EXTRA_SOURCES)
BIN ?= $(NAME)
EXTRA_BINS ?=
BINS := $(BIN) $(EXTRA_BINS)
OUTPUT := $(ROOT_DIR)/target/$(PROFILE)/$(BIN)
OUTPUTS := $(addprefix $(ROOT_DIR)/target/$(PROFILE)/,$(BINS))
DESTDIR ?= /
PREFIX ?= /usr/local
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/$(BIN)
INSTALL_BINS := $(addprefix $(dir $(INSTALL_BIN)),$(BINS))
LINK_BIN := $(HOME)/.local/bin/$(BIN)
LINK_BINS := $(addprefix $(dir $(LINK_BIN)),$(BINS))
ALIASES ?=
INSTALL_ALIASES := $(addprefix $(dir $(INSTALL_BIN)),$(ALIASES))
LINK_ALIASES := $(addprefix $(dir $(LINK_BIN)),$(ALIASES))

.PHONY: all build check install uninstall link unlink test lint fmt clean

all: build

build: $(OUTPUTS)

$(OUTPUTS) &: $(SOURCES)
	@$(CARGO) build -p $(NAME) --profile $(PROFILE)
	@touch $(OUTPUTS)

install: $(INSTALL_BINS) $(INSTALL_ALIASES)

$(INSTALL_BINS): $(dir $(INSTALL_BIN))%: $(ROOT_DIR)/target/$(PROFILE)/%
	@install -d $(@D)
	@install -m 0755 $< $@

$(INSTALL_ALIASES): $(INSTALL_BIN)
	@ln -sf $(BIN) $@

link: $(OUTPUTS)
	@mkdir -p $(dir $(LINK_BIN))
	@$(foreach bin,$(BINS),ln -sf $(ROOT_DIR)/target/$(PROFILE)/$(bin) $(dir $(LINK_BIN))$(bin);)
	@$(foreach alias,$(LINK_ALIASES),ln -sf $(BIN) $(alias);)

unlink:
	@rm -f $(LINK_BINS) $(LINK_ALIASES)

check:
	@cargo fmt -p $(NAME) --check
	@$(CARGO) clippy -p $(NAME) --all-targets -- -D warnings
	@$(CARGO) test -p $(NAME)

test:
	@$(CARGO) test -p $(NAME)

lint:
	@$(CARGO) clippy -p $(NAME) --all-targets -- -D warnings

fmt:
	@cargo fmt -p $(NAME)

uninstall:
	@rm -f $(INSTALL_BINS) $(INSTALL_ALIASES)

clean:
	@$(CARGO) clean -p $(NAME)
