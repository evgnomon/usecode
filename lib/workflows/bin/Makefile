DESTDIR ?= /
PREFIX := $(DESTDIR)usr/local
BIN_DIR := $(PREFIX)/bin

BINS := $(notdir $(shell find $(CURDIR) -maxdepth 1 -type f -executable))
INSTALLED := $(addprefix $(BIN_DIR)/,$(BINS))

.PHONY: build install uninstall clean

build:

$(INSTALLED): $(BIN_DIR)/%: $(CURDIR)/%
	@cp -L --remove-destination $< $@

install: $(INSTALLED)

uninstall:
	@rm -f $(INSTALLED)
