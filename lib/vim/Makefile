# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

.PHONY: build dist clean install update stage

BUILD_DIR     := $(CURDIR)/build
LIB_STAGE     := $(BUILD_DIR)/usr/local
PREFIX        ?= /usr/local
VIM_SRC       := $(CURDIR)/../../third_party/vim/vim
ROOT_DIR      ?= $(abspath $(CURDIR)/../..)
VIM_VERSION   := $(shell git -C $(VIM_SRC) describe --tags --abbrev=0 2>/dev/null | sed 's/^v//')

build: $(VIM_SRC)/.git
	@cd $(VIM_SRC) && BUILD_DIR=$(BUILD_DIR) bash $(CURDIR)/build.sh

$(VIM_SRC)/.git:
	@mkdir -p $(dir $(VIM_SRC))
	@git clone https://github.com/vim/vim.git $(VIM_SRC)

update:
	@git -C $(VIM_SRC) pull --ff-only

stage:

# The uc-vim package: what build staged under build/usr/local.
dist: build
	@uc deb build -v "$(VIM_VERSION)" -o $(ROOT_DIR)/dist

install:
	@cp -r --remove-destination --preserve=mode $(LIB_STAGE)/. $(PREFIX)/

clean:
	@rm -rf $(LIB_STAGE)/bin/vim $(LIB_STAGE)/bin/vimdiff $(LIB_STAGE)/bin/vimtutor \
	       $(LIB_STAGE)/share/vim $(LIB_STAGE)/share/man/man1/vim.1
	@rm -f $(VIM_SRC)/src/auto/config.cache $(VIM_SRC)/src/auto/config.status \
	       $(VIM_SRC)/src/auto/config.h $(VIM_SRC)/src/auto/config.mk
	@$(MAKE) -C $(VIM_SRC) clean 2>/dev/null || true
