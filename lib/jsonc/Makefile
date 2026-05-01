SOURCES := $(shell find . -type f \( -name "*.zig" -o -name "*.c" -o -name "*.h"  \)  ! -path "*/.zig-cache/*")
OUTPUT := zig-out/bin/jsonc
OPTIMIZE ?= ReleaseFast
DESTDIR ?= /
PREFIX ?= /usr/local
INSTALL_BIN := $(DESTDIR)$(patsubst /%,%,$(PREFIX))/bin/jsonc

.PHONY: install clean uninstall build all dist

all: build

build: $(OUTPUT)

$(OUTPUT): $(SOURCES)
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
