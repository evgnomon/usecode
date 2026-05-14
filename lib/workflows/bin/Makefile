ROOT_DIR ?= $(abspath $(CURDIR)/../../..)
BUILD_DIR ?= $(ROOT_DIR)/build
DESTDIR ?= /
STAMP_DIR := $(CURDIR)/.stamps
BIN_DIR := $(BUILD_DIR)/usr/local/bin
INSTALL_BIN_DIR := $(DESTDIR)usr/local/bin
BUILD_STAMP := $(STAMP_DIR)/build

BINS := annabelle ansidist barge boom catalyze certgen confmap diga docker ensure_secret ensure_vault extract-tool ff getsecret ghchain git_repos gotorepo imgpress ipmesh k8s_ddl.sh keychain mgo mkarole mkp12 mkpod mkunit natsup num_argv plat pubdeb rchain repofqn rotate_keychain_pass rotsec secd set_git_conf shole sqlize trust_ca vi wdoctl whcloud x y yacht ykattach ysys zcdump
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
	cp -L --remove-destination $< $@

uninstall:
	rm -f $(INSTALLED)

clean:
	rm -f $(LINKS)
	rm -rf $(STAMP_DIR)
