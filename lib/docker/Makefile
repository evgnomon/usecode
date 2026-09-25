# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := docker
SOURCES := $(shell find src -type f -name "*.rs") Cargo.toml Cargo.lock
PROFILE ?= release
OUTPUT := target/$(PROFILE)/$(NAME)
DESTDIR ?= /
PREFIX ?= /usr/local
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/$(NAME)
LINK_BIN := $(HOME)/.local/bin/$(NAME)

.PHONY: all build check install uninstall link unlink test lint fmt clean

all: build

build: $(OUTPUT)

$(OUTPUT): $(SOURCES)
	@cargo build --profile $(PROFILE)
	@touch $(OUTPUT)

Cargo.lock:
	@cargo generate-lockfile

install: $(INSTALL_BIN)

$(INSTALL_BIN): $(OUTPUT)
	@install -d $(@D)
	@install -m 0755 $(OUTPUT) $@

link: $(OUTPUT)
	@mkdir -p $(dir $(LINK_BIN))
	@ln -sf $(CURDIR)/$(OUTPUT) $(LINK_BIN)

unlink:
	@rm -f $(LINK_BIN)

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

uninstall:
	@rm -f $(INSTALL_BIN)

clean:
	@cargo clean
