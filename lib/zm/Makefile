SOURCES := $(shell find . -type f \( -name "*.zig" -o -name "*.c" -o -name "*.h"  \)  ! -path "*/.zig-cache/*")
OUTPUT := zig-out/bin/zm
OPTIMIZE ?= ReleaseFast
ENV_FILE := .env
DESTDIR ?= /
PREFIX ?= /usr
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/zm

-include $(ENV_FILE)

.PHONY: install clean uninstall build all dist

.env:
	@touch .env

all: build

build: $(OUTPUT)

$(OUTPUT): $(SOURCES) $(ENV_FILE)
	@zig build -Doptimize=$(OPTIMIZE)
	@touch $(OUTPUT)

install: $(INSTALL_BIN)

$(INSTALL_BIN): $(OUTPUT)
	@install -d $(@D)
	@install -m 0755 $(OUTPUT) $@

dist: build
	@mkdeb

publish: dist
	@pubdeb

uninstall:
	@rm -f $(INSTALL_BIN)

clean:
	@rm -rf $(OUTPUT)
