.PHONY: clean install uninstall help

DESTDIR ?= /
INSTALL_PREFIX ?= /usr/local

ROOT_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_DIR ?= $(ROOT_DIR)/build
BUILD_PREFIX := $(BUILD_DIR)/usr/local
LIB_BUILD_DIR := $(CURDIR)/build
LIB_BUILD_PREFIX := $(LIB_BUILD_DIR)/usr/local
STAMP_DIR := $(ROOT_DIR)/.stamps/$(patsubst $(ROOT_DIR)/%,%,$(CURDIR))

PYENV_ROOT ?= $(HOME)/.pyenv
PYTHON_VERSION ?= 3.14.3
PYENV_PYTHON := $(PYENV_ROOT)/versions/$(PYTHON_VERSION)
PY_MAJMIN := $(basename $(PYTHON_VERSION))

COPY := cp -L --preserve=mode,timestamps --remove-destination
LINK := ln -sf

PY_BINS := \
	pip \
	pip3 \
	pip3.14 \
	pydoc \
	pydoc3 \
	pydoc3.14 \
	python \
	python3 \
	python3.14 \
	python3.14-config \
	python3-config \
	python-config \
	python3.14-gdb.py

LIB_STAGED_BINS := $(addprefix $(LIB_BUILD_PREFIX)/bin/,$(PY_BINS))
STAGED_BINS := $(addprefix $(BUILD_PREFIX)/bin/,$(PY_BINS))

INSTALL_BIN_DIR := $(DESTDIR)$(patsubst /%,%,$(INSTALL_PREFIX))/bin
INSTALL_BINS := $(addprefix $(INSTALL_BIN_DIR)/,$(PY_BINS))

build: $(STAGED_BINS)

$(LIB_BUILD_PREFIX)/bin/%: $(PYENV_PYTHON)/bin/%
	@mkdir -p $(dir $@)
	@$(COPY) $< $@

$(BUILD_PREFIX)/bin/%: $(LIB_STAGED_BINS)
	@mkdir -p $(dir $@)
	@$(LINK) $(abspath $<) $@

install: $(INSTALL_BINS) ## Install bins from $(BUILD_PREFIX)/bin into $(DESTDIR)$(INSTALL_PREFIX)/bin

$(INSTALL_BIN_DIR)/%: $(BUILD_PREFIX)/bin/%
	@mkdir -p $(INSTALL_BIN_DIR)
	@$(COPY) $< $@

uninstall: ## Remove installed bins from $(DESTDIR)$(INSTALL_PREFIX)/bin
	@rm -f $(INSTALL_BINS)

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

.DEFAULT_GOAL := build
