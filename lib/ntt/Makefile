SOURCES := $(shell find . -type f \( -name "*.zig" -o -name "*.c" -o -name "*.h"  \)  ! -path "*/.zig-cache/*")
BINNAME := ntt
OUTPUT := zig-out/bin/$(BINNAME)
OPTIMIZE ?= ReleaseFast
ENV_FILE := .env

-include $(ENV_FILE)

.PHONY: install clean uninstall build all dist

.env:
	@touch .env

all: dist

build: $(OUTPUT)

$(OUTPUT): $(SOURCES) $(ENV_FILE)
	@zig build -Doptimize=$(OPTIMIZE)
	@touch $(OUTPUT)

install: build
	@cp $(OUTPUT) /usr/bin/$(BINNAME)

dist: build
	@mkdeb

publish: dist
	@pubdeb

uninstall:
	@rm /usr/bin/$(BINNAME)

clean:
	@rm -rf $(OUTPUT)
