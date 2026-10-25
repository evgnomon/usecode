# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

ROOT_DIR ?= $(abspath ../..)
SOURCES := $(shell find src tests -type f -name "*.rs") Cargo.toml $(ROOT_DIR)/Cargo.lock
include $(ROOT_DIR)/lib/profile.mk
# The machine's own arch; TARGET=aarch64-unknown-linux-musl cross-builds for arm.
TARGET ?= $(shell uname -m)-unknown-linux-musl
BINS := uc uc-secret-encrypt uc-secret-decrypt uc-sync uc-kube-sync uc-image-push uc-image-pull uc-secret \
	uc-image uc-cert uc-deb uc-db uc-net uc-daemon uc-vm uc-new \
	uc-agent uc-profile uc-terminal uc-mail uc-mail-configure uc-mail-settings uc-mail-domain uc-mail-account uc-mail-pull uc-mail-view uc-mail-send uc-dns uc-fs uc-kube-logs uc-kube-usage uc-forward uc-workspace
OUT_DIR := $(ROOT_DIR)/target/$(TARGET)/$(PROFILE)
OUTPUTS := $(addprefix $(OUT_DIR)/,$(BINS))
DESTDIR ?= /
PREFIX ?= /usr/local
BIN_DIR := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin
INSTALLED := $(addprefix $(BIN_DIR)/,$(BINS))
# Former standalone tools that uc-secret still answers to by name.
SECRET_ALIASES := getsecret keychain rchain ghchain ensure_secret ensure_vault rotate_keychain_pass
INSTALLED_ALIASES := $(addprefix $(BIN_DIR)/,$(SECRET_ALIASES))
VERSION ?= $(shell git describe --tags --match 'v*' --abbrev=0 2>/dev/null | sed 's/^v//')
DEB_ROOT := $(CURDIR)/pkg/debian

.PHONY: all build check install uninstall test lint fmt deb dist publish clean

all: build

build: $(OUTPUTS)

$(OUTPUTS) &: $(SOURCES)
	@$(CARGO) build -p uc --profile $(PROFILE) --target $(TARGET)
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

# One package for uc and all its commands: the uc-* binaries built here and
# by every lib/* crate (deb-stage in lib/rust.mk), packed by the uc just
# built, so uc deb runs the uc-deb-build next to it.
deb: build
	@rm -rf $(DEB_ROOT)
	@install -d $(DEB_ROOT)/usr/bin
	@install -m 0755 $(OUTPUTS) $(DEB_ROOT)/usr/bin
	@cd $(ROOT_DIR) && DEB_ROOT=$(DEB_ROOT) MAKE=$(MAKE) ./scripts/each_lib.sh deb-stage
	@$(DEB_ROOT)/usr/bin/uc deb build -v "$(VERSION)" -o $(ROOT_DIR)/dist

dist: deb

publish: dist
	@cd $(ROOT_DIR) && $(DEB_ROOT)/usr/bin/uc deb publish

uninstall:
	@rm -f $(INSTALLED) $(INSTALLED_ALIASES)

clean:
	@$(CARGO) clean -p uc
