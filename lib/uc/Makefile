# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

SOURCES := $(shell find src tests -type f -name "*.rs") Cargo.toml Cargo.lock
PROFILE ?= release
TARGET ?= x86_64-unknown-linux-musl
BINS := uc uc-encrypt uc-decrypt uc-configure uc-push uc-pull uc-ghcr uc-secret
OUT_DIR := target/$(TARGET)/$(PROFILE)
OUTPUTS := $(addprefix $(OUT_DIR)/,$(BINS))
DESTDIR ?= /
PREFIX ?= /usr/local
BIN_DIR := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin
INSTALLED := $(addprefix $(BIN_DIR)/,$(BINS))

.PHONY: all build check install uninstall test lint fmt dist publish clean

all: build

build: $(OUTPUTS)

$(OUTPUTS) &: $(SOURCES)
	@scripts/build.sh $(PROFILE) $(TARGET)
	@touch $(OUTPUTS)

install: $(INSTALLED)

$(INSTALLED): $(BIN_DIR)/%: $(OUT_DIR)/%
	@install -d $(@D)
	@install -m 0755 $< $@

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
	@rm -f $(INSTALLED)

clean:
	@cargo clean
