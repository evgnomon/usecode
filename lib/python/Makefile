.PHONY: build clean clean-stamps clean-stamp-configure clean-stamp-build clean-stamp-install clean-stamp-link install uninstall help

DESTDIR ?= /
INSTALL_PREFIX ?= /usr/local

ROOT_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_DIR ?= $(ROOT_DIR)/build
BUILD_PREFIX := $(BUILD_DIR)/usr/local
LIB_BUILD_DIR := $(CURDIR)/build
LIB_BUILD_PREFIX := $(LIB_BUILD_DIR)/usr/local
STAMP_DIR := $(ROOT_DIR)/.stamps/$(patsubst $(ROOT_DIR)/%,%,$(CURDIR))

PYTHON_VERSION ?= 3.14.3
PY_MAJMIN := $(basename $(PYTHON_VERSION))
CPYTHON_SRC := $(ROOT_DIR)/third_party/python/cpython

$(STAMP_DIR)/configure.$(PYTHON_VERSION): $(CPYTHON_SRC)/.git/HEAD
	@mkdir -p $(dir $@)
	git -C $(CPYTHON_SRC) fetch --depth 1 origin refs/tags/v$(PYTHON_VERSION):refs/tags/v$(PYTHON_VERSION)
	git -C $(CPYTHON_SRC) checkout v$(PYTHON_VERSION)
	$(MAKE) -C $(CPYTHON_SRC) distclean 2>/dev/null || true
	cd $(CPYTHON_SRC) && ./configure \
		--prefix=$(LIB_BUILD_PREFIX) \
		--enable-shared \
		--enable-optimizations \
		--with-lto \
		--with-ensurepip=install
	@touch $@

$(STAMP_DIR)/build.$(PYTHON_VERSION): $(STAMP_DIR)/configure.$(PYTHON_VERSION)
	$(MAKE) -C $(CPYTHON_SRC) -j$$(nproc)
	@touch $@

$(STAMP_DIR)/install.$(PYTHON_VERSION): $(STAMP_DIR)/build.$(PYTHON_VERSION)
	$(MAKE) -C $(CPYTHON_SRC) install
	@touch $@

BIN_LINKS := \
	bin/idle3 \
	bin/idle$(PY_MAJMIN) \
	bin/pip3 \
	bin/pip$(PY_MAJMIN) \
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
DST_LINKS   = $(addprefix $(BUILD_PREFIX)/,$(LINK_ITEMS))

$(BUILD_PREFIX)/%: $(LIB_BUILD_PREFIX)/% | $(STAMP_DIR)/install.$(PYTHON_VERSION)
	@mkdir -p $(@D)
	ln -sfn $$(realpath --relative-to=$(@D) $<) $@

$(STAMP_DIR)/link.$(PYTHON_VERSION): $(STAMP_DIR)/install.$(PYTHON_VERSION) $(DST_LINKS)
	@touch $@

build: $(STAMP_DIR)/link.$(PYTHON_VERSION) ## Build CPython from source and link into build prefix

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
