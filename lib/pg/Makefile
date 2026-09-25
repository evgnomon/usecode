SOURCES := $(shell find src -type f -name "*.rs") Cargo.toml Cargo.lock
PROFILE ?= release
OUTPUT := target/$(PROFILE)/pg
DESTDIR ?= /
PREFIX ?= /usr/local
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/pg
LINK_BIN := $(HOME)/.local/bin/pg

.PHONY: all build install uninstall link unlink test lint fmt clean

all: build

build: $(OUTPUT)

$(OUTPUT): $(SOURCES)
	@cargo build --profile $(PROFILE)
	@touch $(OUTPUT)

install: $(INSTALL_BIN)

$(INSTALL_BIN): $(OUTPUT)
	@install -d $(@D)
	@install -m 0755 $(OUTPUT) $@

link: $(OUTPUT)
	@mkdir -p $(dir $(LINK_BIN))
	@ln -sf $(CURDIR)/$(OUTPUT) $(LINK_BIN)

unlink:
	@rm -f $(LINK_BIN)

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
