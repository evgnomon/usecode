ROOT_DIR ?= $(abspath $(CURDIR)/../../..)
BUILD_DIR ?= $(ROOT_DIR)/build
DESTDIR ?= /
STAMP_DIR := $(ROOT_DIR)/.stamps/$(patsubst $(ROOT_DIR)/%,%,$(CURDIR))
BIN_DIR := $(BUILD_DIR)/usr/local/bin
INSTALL_BIN_DIR := $(DESTDIR)usr/local/bin
BUILD_STAMP := $(STAMP_DIR)/build

BINS := catalyze eth extract-tool git_repos set_git_conf
LINKS := $(addprefix $(BIN_DIR)/,$(BINS))
INSTALLED := $(addprefix $(INSTALL_BIN_DIR)/,$(BINS))

.PHONY: build install uninstall clean

build: $(BUILD_STAMP)

$(BUILD_STAMP): $(LINKS)
	@mkdir -p $(STAMP_DIR)
	@touch $@

$(BIN_DIR)/%: $(CURDIR)/%
	@mkdir -p $(BIN_DIR)
	ln -sfn $< $@

install: $(INSTALLED)

$(INSTALL_BIN_DIR)/%: $(BIN_DIR)/%
	@mkdir -p $(INSTALL_BIN_DIR)
	cp -L $< $@

uninstall:
	rm -f $(INSTALLED)

clean:
	rm -f $(LINKS)
	rm -rf $(STAMP_DIR)
