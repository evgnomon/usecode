# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

ROOT_DIR ?= $(abspath ../..)
SOURCES := $(shell find src tests -type f -name "*.rs") Cargo.toml $(ROOT_DIR)/Cargo.lock
include $(ROOT_DIR)/lib/profile.mk
TARGET ?= x86_64-unknown-linux-musl
BINS := uc uc-secret-encrypt uc-secret-decrypt uc-configure uc-push uc-pull uc-ghcr uc-secret \
	uc-image uc-repo uc-cert uc-deb uc-db uc-net uc-daemon uc-vm uc-new uc-cloud \
	uc-agent uc-data uc-media uc-pick uc-sys
OUT_DIR := $(ROOT_DIR)/target/$(TARGET)/$(PROFILE)
OUTPUTS := $(addprefix $(OUT_DIR)/,$(BINS))
DESTDIR ?= /
PREFIX ?= /usr/local
BIN_DIR := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin
INSTALLED := $(addprefix $(BIN_DIR)/,$(BINS))
# Former standalone tools that uc-secret still answers to by name.
SECRET_ALIASES := getsecret keychain rchain ghchain ensure_secret ensure_vault rotate_keychain_pass rotsec
INSTALLED_ALIASES := $(addprefix $(BIN_DIR)/,$(SECRET_ALIASES))

.PHONY: all build check install uninstall test lint fmt dist publish clean

all: build

build: $(OUTPUTS)

$(OUTPUTS) &: $(SOURCES)
	@scripts/build.sh $(PROFILE) $(TARGET)
	@touch $(OUTPUTS)

install: $(INSTALLED) $(INSTALLED_ALIASES)

$(INSTALLED): $(BIN_DIR)/%: $(OUT_DIR)/%
	@install -d $(@D)
	@install -m 0755 $< $@

$(INSTALLED_ALIASES): $(BIN_DIR)/uc-secret
	@ln -sf uc-secret $@

check:
	@cargo fmt -p uc --check
	@$(CARGO) clippy -p uc --all-targets -- -D warnings
	@$(CARGO) test -p uc

test:
	@$(CARGO) test -p uc

lint:
	@$(CARGO) clippy -p uc --all-targets -- -D warnings

fmt:
	@cargo fmt -p uc

dist: build
	@uc-deb-build

publish: dist
	@uc-deb-publish

uninstall:
	@rm -f $(INSTALLED) $(INSTALLED_ALIASES)

clean:
	@$(CARGO) clean -p uc
