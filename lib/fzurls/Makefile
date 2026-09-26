# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := fzurls
SOURCES := $(shell find src -type f -name "*.rs") Cargo.toml Cargo.lock
PROFILE ?= release
OUTPUT := target/$(PROFILE)/$(NAME)
DESTDIR ?= /
PREFIX ?= /usr/local
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/$(NAME)
LINK_BIN := $(HOME)/.local/bin/$(NAME)
EXT_UUID := fzurls-focus@org.evgnomon
EXT_DIR := $(HOME)/.local/share/gnome-shell/extensions/$(EXT_UUID)
EXT_SRC_DIR := gnome-ext
EXT_FILES := extension.js metadata.json
INSTALLED_EXT_FILES := $(addprefix $(EXT_DIR)/,$(EXT_FILES))

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

link: $(OUTPUT) $(INSTALLED_EXT_FILES)
	@mkdir -p $(dir $(LINK_BIN))
	@ln -sf $(CURDIR)/$(OUTPUT) $(LINK_BIN)
	gsettings set org.gnome.shell disable-user-extensions false 2>/dev/null || true
	gnome-extensions enable $(EXT_UUID) 2>/dev/null || echo "Restart GNOME Shell, then run: gnome-extensions enable $(EXT_UUID)"

$(EXT_DIR)/%: $(EXT_SRC_DIR)/%
	install -d $(@D)
	install -m 0644 $< $@

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
	@rm -f $(INSTALL_BIN) $(LINK_BIN)
	gnome-extensions disable $(EXT_UUID) 2>/dev/null || true
	rm -f $(INSTALLED_EXT_FILES)
	rmdir $(EXT_DIR) 2>/dev/null || true

clean:
	@cargo clean
