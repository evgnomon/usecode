.PHONY: build build-deps verify clean clean-stamps clean-stamp-configure clean-stamp-build clean-stamp-install clean-stamp-link install uninstall help

DESTDIR ?= /
INSTALL_PREFIX ?= /usr/local

ROOT_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_DIR ?= $(ROOT_DIR)/build
PREFIX := $(BUILD_DIR)/usr/local
LIB_BUILD_DIR := $(CURDIR)/build
LIB_BUILD_PREFIX := $(LIB_BUILD_DIR)/usr/local
STAMP_DIR := $(CURDIR)/.stamps

PYTHON_VERSION ?= $(shell cat .deb.jsonc | jsonc | jq -r '.version')
PY_MAJMIN := $(basename $(PYTHON_VERSION))
CPYTHON_SRC := $(ROOT_DIR)/third_party/python/cpython

APT_BUILD_DEPS := \
	build-essential pkg-config \
	libssl-dev zlib1g-dev libbz2-dev liblzma-dev \
	libffi-dev libsqlite3-dev libreadline-dev \
	libncurses-dev tk-dev uuid-dev \
	libgdbm-dev libgdbm-compat-dev libexpat1-dev

# stdlib modules that must import in a full-flag build
VERIFY_MODULES := ssl hashlib zlib bz2 lzma ctypes sqlite3 readline \
	curses _curses_panel dbm.gnu dbm.ndbm uuid tkinter decimal \
	xml.parsers.expat _socket zoneinfo

build-deps: ## Install Debian build-time deps for a full-featured CPython
	sudo apt-get update
	sudo apt-get install -y --no-install-recommends $(APT_BUILD_DEPS)

$(STAMP_DIR)/configure.$(PYTHON_VERSION):
	@mkdir -p $(dir $@)
	@missing=""; for p in $(APT_BUILD_DEPS); do \
		dpkg-query -W -f='$${Status}' $$p 2>/dev/null | grep -q "install ok installed" || missing="$$missing $$p"; \
	done; \
	if [ -n "$$missing" ]; then \
		echo "ERROR: missing build deps:$$missing"; \
		echo "Run: make build-deps"; \
		exit 1; \
	fi
	git -C $(CPYTHON_SRC) fetch --depth 1 origin refs/tags/v$(PYTHON_VERSION):refs/tags/v$(PYTHON_VERSION)
	git -C $(CPYTHON_SRC) checkout v$(PYTHON_VERSION)
	$(MAKE) -C $(CPYTHON_SRC) distclean 2>/dev/null || true
	cd $(CPYTHON_SRC) && ./configure \
		--prefix=$(INSTALL_PREFIX) \
		--enable-shared \
		--enable-optimizations \
		--with-lto \
		--with-ensurepip=install \
		--enable-loadable-sqlite-extensions \
		--with-dbmliborder=gdbm:ndbm \
		--with-readline=readline \
		--with-computed-gotos
	@touch $@

$(STAMP_DIR)/build.$(PYTHON_VERSION): $(STAMP_DIR)/configure.$(PYTHON_VERSION)
	$(MAKE) -C $(CPYTHON_SRC) -j$$(nproc)
	@touch $@

$(STAMP_DIR)/install.$(PYTHON_VERSION): $(STAMP_DIR)/build.$(PYTHON_VERSION)
	$(MAKE) -C $(CPYTHON_SRC) install DESTDIR=$(LIB_BUILD_DIR)
	@PYTHONHOME=$(LIB_BUILD_PREFIX) \
	 LD_LIBRARY_PATH=$(LIB_BUILD_PREFIX)/lib \
	 $(LIB_BUILD_PREFIX)/bin/python$(PY_MAJMIN) -c \
	 "import sys, importlib.util; missing=[m for m in '$(VERIFY_MODULES)'.split() if not importlib.util.find_spec(m)]; sys.exit('MISSING MODULES: '+' '.join(missing)) if missing else print('all modules present')"
	@touch $@

verify: $(STAMP_DIR)/install.$(PYTHON_VERSION) ## Re-run the stdlib module verification
	@PYTHONHOME=$(LIB_BUILD_PREFIX) \
	 LD_LIBRARY_PATH=$(LIB_BUILD_PREFIX)/lib \
	 $(LIB_BUILD_PREFIX)/bin/python$(PY_MAJMIN) -c \
	 "import sys, importlib.util; missing=[m for m in '$(VERIFY_MODULES)'.split() if not importlib.util.find_spec(m)]; sys.exit('MISSING MODULES: '+' '.join(missing)) if missing else print('all modules present')"

BIN_LINKS := \
	bin/idle3 \
	bin/idle$(PY_MAJMIN) \
	bin/pydoc3 \
	bin/pydoc$(PY_MAJMIN) \
	bin/python3 \
	bin/python3-config \
	bin/python$(PY_MAJMIN) \
	bin/python$(PY_MAJMIN)-config

LIB_LINKS := \
	lib/libpython$(PY_MAJMIN).so \
	lib/libpython$(PY_MAJMIN).so.1.0 \
	lib/libpython3.so \
	lib/pkgconfig \
	lib/python$(PY_MAJMIN)

INCLUDE_LINKS := \
	include/python$(PY_MAJMIN)

MAN_LINKS = $(patsubst $(LIB_BUILD_PREFIX)/%,%,$(wildcard $(LIB_BUILD_PREFIX)/share/man/man1/python*))

LINK_ITEMS = $(BIN_LINKS) $(LIB_LINKS) $(INCLUDE_LINKS) $(MAN_LINKS)
DST_LINKS   = $(addprefix $(PREFIX)/,$(LINK_ITEMS))

$(PREFIX)/%: $(LIB_BUILD_PREFIX)/% | $(STAMP_DIR)/install.$(PYTHON_VERSION)
	@mkdir -p $(@D)
	@ln -sfn $$(realpath --relative-to=$(@D) $<) $@


$(STAMP_DIR)/link.$(PYTHON_VERSION): $(STAMP_DIR)/install.$(PYTHON_VERSION) $(DST_LINKS)
	@touch $@

build: $(STAMP_DIR)/link.$(PYTHON_VERSION) ## Build CPython from source and link into build prefix

dist: 
	@mkdeb

install: ## Copy Python files into DESTDIR/INSTALL_PREFIX
	@mkdir -p $(DESTDIR)$(patsubst /%,%,$(INSTALL_PREFIX))
	cp -aL --remove-destination $(LIB_BUILD_PREFIX)/. $(DESTDIR)$(patsubst /%,%,$(INSTALL_PREFIX))/

uninstall: ## Remove Python from DESTDIR/INSTALL_PREFIX
	@rm -rf \
		$(DESTDIR)$(patsubst /%,%,$(INSTALL_PREFIX))/bin/python* \
		$(DESTDIR)$(patsubst /%,%,$(INSTALL_PREFIX))/bin/pip* \
		$(DESTDIR)$(patsubst /%,%,$(INSTALL_PREFIX))/bin/pydoc* \
		$(DESTDIR)$(patsubst /%,%,$(INSTALL_PREFIX))/lib/python$(PY_MAJMIN)* \
		$(DESTDIR)$(patsubst /%,%,$(INSTALL_PREFIX))/lib/libpython*

clean: clean-stamps ## Remove build artifacts and stamps
	@rm -rf $(LIB_BUILD_DIR)

clean-stamps: ## Remove all stamps (forces re-run of all build phases)
	@rm -rf $(STAMP_DIR)

clean-stamp-configure: ## Remove configure stamp (forces re-configure)
	@rm -f $(STAMP_DIR)/configure.$(PYTHON_VERSION)

clean-stamp-build: ## Remove build stamp (forces recompile)
	@rm -f $(STAMP_DIR)/build.$(PYTHON_VERSION)

clean-stamp-install: ## Remove install stamp (forces reinstall)
	@rm -f $(STAMP_DIR)/install.$(PYTHON_VERSION)

clean-stamp-link: ## Remove link stamp (forces re-link into build prefix)
	@rm -f $(STAMP_DIR)/link.$(PYTHON_VERSION)

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

.DEFAULT_GOAL := build
