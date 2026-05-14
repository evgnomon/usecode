ROOT_DIR ?= $(abspath $(CURDIR)/../../..)
BUILD_DIR ?= $(ROOT_DIR)/build
DESTDIR ?= /
STAMP_DIR := $(CURDIR)/.stamps
BIN_DIR := $(BUILD_DIR)/usr/local/bin
INSTALL_BIN_DIR := $(DESTDIR)usr/local/bin
BUILD_STAMP := $(STAMP_DIR)/build

BINS := $(notdir $(shell find $(CURDIR) -maxdepth 1 -type f -executable))
LINKS := $(addprefix $(BIN_DIR)/,$(BINS))
INSTALLED := $(addprefix $(INSTALL_BIN_DIR)/,$(BINS))

.PHONY: build install uninstall clean

build: $(BUILD_STAMP)

$(BUILD_STAMP): $(LINKS)
	@mkdir -p $(STAMP_DIR)
	@touch $@

$(BIN_DIR)/%: $(CURDIR)/%
	@mkdir -p $(BIN_DIR)
	@ln -sfn $< $@

install: $(INSTALLED)

$(INSTALL_BIN_DIR)/%: $(BIN_DIR)/%
	@mkdir -p $(INSTALL_BIN_DIR)
	@cp -L --remove-destination $< $@

uninstall:
	@rm -f $(INSTALLED)

clean:
	@rm -f $(LINKS)
	@rm -rf $(STAMP_DIR)
