SOURCES := $(shell find . -type f \( -name "*.zig" -o -name "*.c" -o -name "*.h"  \)  ! -path "*/.zig-cache/*")
BINNAME := ntt
OUTPUT := zig-out/bin/$(BINNAME)
ABS_OUTPUT := $(abspath $(OUTPUT))
OPTIMIZE ?= ReleaseFast
ENV_FILE := .env
BUILD_DIR ?= $(abspath $(CURDIR)/../../build)
VM_LINK := $(BUILD_DIR)/usr/local/bin/vm

-include $(ENV_FILE)

.PHONY: install clean uninstall build all dist

.env:
	@touch .env

all: build

build: $(OUTPUT) $(VM_LINK)

$(OUTPUT): $(SOURCES) $(ENV_FILE)
	@zig build -Doptimize=$(OPTIMIZE)
	@touch $(OUTPUT)

$(VM_LINK): $(OUTPUT)
	@mkdir -p $(dir $(VM_LINK))
	@ln -sf $(ABS_OUTPUT) $(VM_LINK)

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
	@rm -rf .zig-cache
