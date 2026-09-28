# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

.PHONY: build build-deps clean install update link unlink

BUILD_DIR      := $(CURDIR)/build
LIB_STAGE      := $(BUILD_DIR)/usr/local
PREFIX         ?= /usr/local
ALACRITTY_SRC  := $(CURDIR)/../../third_party/alacritty/alacritty
CONFIG_LINK    := $(HOME)/.config/alacritty/alacritty.toml

APT_BUILD_DEPS := cmake g++ pkg-config libfreetype-dev libfontconfig1-dev \
	libxcb-xfixes0-dev libxkbcommon-dev python3 scdoc gzip ncurses-bin \
	desktop-file-utils

build: $(ALACRITTY_SRC)/.git
	@cd $(ALACRITTY_SRC) && BUILD_DIR=$(BUILD_DIR) bash $(CURDIR)/build.sh

$(ALACRITTY_SRC)/.git:
	@mkdir -p $(dir $(ALACRITTY_SRC))
	@git clone https://github.com/alacritty/alacritty.git $(ALACRITTY_SRC)

build-deps:
	sudo apt-get update
	sudo apt-get install -y --no-install-recommends $(APT_BUILD_DEPS)

update:
	@git -C $(ALACRITTY_SRC) pull --ff-only

link:
	@mkdir -p $(dir $(CONFIG_LINK))
	@ln -sfn $(CURDIR)/alacritty.toml $(CONFIG_LINK)

unlink:
	@rm -f $(CONFIG_LINK)

install:
	@cp -r --preserve=mode $(LIB_STAGE)/. $(PREFIX)/
	@PREFIX=$(PREFIX) bash $(CURDIR)/install.sh

clean:
	@rm -rf $(BUILD_DIR)
	@cargo clean --manifest-path $(ALACRITTY_SRC)/Cargo.toml 2>/dev/null || true
