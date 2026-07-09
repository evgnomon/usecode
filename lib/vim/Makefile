.PHONY: build clean install init update stage link unlink

BUILD_DIR     := $(CURDIR)/build
LIB_STAGE     := $(BUILD_DIR)/usr/local
PREFIX        ?= /usr/local
VIM_SRC       := $(CURDIR)/../../third_party/vim/vim

LINK_FILES    := .vimrc .ideavimrc .vim/coc-settings.json
LINKS         := $(addprefix $(HOME)/,$(LINK_FILES))

build: $(VIM_SRC)/.git
	@cd $(VIM_SRC) && BUILD_DIR=$(BUILD_DIR) bash $(CURDIR)/build.sh

$(VIM_SRC)/.git:
	@mkdir -p $(dir $(VIM_SRC))
	@git clone https://github.com/vim/vim.git $(VIM_SRC)

update:
	@git -C $(VIM_SRC) pull --ff-only

stage:

link: $(LINKS)

$(HOME)/.vim:
	@mkdir -p $@

$(LINKS): $(HOME)/%: $(CURDIR)/%
	@ln -sf $< $@

unlink:
	@for f in $(LINKS); do \
		if [ -L "$$f" ]; then rm -f "$$f"; fi; \
	done

install:
	@cp -r --preserve=mode $(LIB_STAGE)/. $(PREFIX)/

clean:
	@rm -rf $(LIB_STAGE)/bin/vim $(LIB_STAGE)/bin/vimdiff $(LIB_STAGE)/bin/vimtutor \
	       $(LIB_STAGE)/share/vim $(LIB_STAGE)/share/man/man1/vim.1
	@rm -f $(VIM_SRC)/src/auto/config.cache $(VIM_SRC)/src/auto/config.status \
	       $(VIM_SRC)/src/auto/config.h $(VIM_SRC)/src/auto/config.mk
	@$(MAKE) -C $(VIM_SRC) clean 2>/dev/null || true

init: | $(HOME)/.vim
init: link
	@plug="$(HOME)/.vim/autoload/plug.vim"; \
	if [ ! -f "$$plug" ]; then \
		curl -fLo "$$plug" --create-dirs \
		    https://raw.githubusercontent.com/junegunn/vim-plug/master/plug.vim; \
	fi
	@mkdir -p $(HOME)/.vim/plugin
	@ln -sf $(CURDIR)/coc-settings.json $(HOME)/.vim/coc-settings.json
