.PHONY: build clean install init update link

ROOT_DIR      ?= $(abspath $(CURDIR)/../../..)
BUILD_DIR     ?= $(ROOT_DIR)/build
LIB_BUILD_DIR := $(CURDIR)/build
LIB_STAGE     := $(LIB_BUILD_DIR)/usr/local
PREFIX        ?= /usr/local
THIRD_PARTY   := $(ROOT_DIR)/third_party
VIM_SRC       := $(THIRD_PARTY)/vim/vim

build: $(VIM_SRC)/.git
	@cd $(VIM_SRC) && BUILD_DIR=$(LIB_BUILD_DIR) bash $(CURDIR)/build.sh
	@$(MAKE) link

$(VIM_SRC)/.git:
	@mkdir -p $(dir $(VIM_SRC))
	@git clone https://github.com/vim/vim.git $(VIM_SRC)

update:
	@git -C $(VIM_SRC) pull --ff-only

link:
	@find $(LIB_BUILD_DIR) \( -type f -o -type l \) 2>/dev/null | while IFS= read -r src; do \
		rel="$${src#$(LIB_BUILD_DIR)/}"; \
		dest="$(BUILD_DIR)/$$rel"; \
		mkdir -p "$$(dirname "$$dest")"; \
		ln -srf "$$src" "$$dest"; \
	done

install:
	@cp -a $(LIB_STAGE)/. $(PREFIX)/

clean:
	@rm -rf $(LIB_STAGE)/bin/vim $(LIB_STAGE)/bin/vimdiff $(LIB_STAGE)/bin/vimtutor \
	       $(LIB_STAGE)/share/vim $(LIB_STAGE)/share/man/man1/vim.1
	@rm -f $(VIM_SRC)/src/auto/config.cache $(VIM_SRC)/src/auto/config.status \
	       $(VIM_SRC)/src/auto/config.h $(VIM_SRC)/src/auto/config.mk
	@$(MAKE) -C $(VIM_SRC) clean 2>/dev/null || true

init:
	@plug="$(HOME)/.vim/autoload/plug.vim"; \
	if [ ! -f "$$plug" ]; then \
		curl -fLo "$$plug" --create-dirs \
		    https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim; \
	fi
	@ln -sf $(CURDIR)/.vimrc          $(HOME)/.vimrc
	@ln -sf $(CURDIR)/.ideavimrc      $(HOME)/.ideavimrc
	@mkdir -p $(HOME)/.vim
	@ln -sf $(CURDIR)/coc-settings.json $(HOME)/.vim/coc-settings.json
