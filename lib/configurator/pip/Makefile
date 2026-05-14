.PHONY: all install upgrade clean

NO_UPGRADE ?= 0

STAMPS_DIR := .stamps
BUILD_DIR  := $(CURDIR)/build
PREFIX			:= $(BUILD_DIR)/usr/local

SPEC := .deb.jsonc
VERSION := $(shell cat .deb.jsonc | jsonc | jq -r '.version')

DEB_PKG := dist/ppkgs_$(VERSION)_amd64.deb

PIP_PACKAGES  := $(shell grep -v '^\s*#' requirements.txt | grep -v '^\s*$$' | tr '\n' ' ')
PIPX_PACKAGES := $(shell grep -v '^\s*#' pipx-requirements.txt | grep -v '^\s*$$' | tr '\n' ' ')

ifeq ($(NO_UPGRADE),1)
_pip_stamp  := $(STAMPS_DIR)/pip-install
_pipx_stamp := $(STAMPS_DIR)/pipx-install
else
_pip_stamp  := $(STAMPS_DIR)/pip-upgrade
_pipx_stamp := $(STAMPS_DIR)/pipx-upgrade
endif

all: $(_pip_stamp) $(_pipx_stamp) $(STAMPS_DIR)/pyenv-rehash $(DEB_PKG)

$(STAMPS_DIR):
	@mkdir -p $@

$(PREFIX):
	@mkdir -p $@

$(STAMPS_DIR)/pip-install: requirements.txt | $(STAMPS_DIR) $(PREFIX)
	python -m pip install --prefix $(PREFIX) --ignore-installed $(PIP_PACKAGES)
	@touch $@

$(STAMPS_DIR)/pip-upgrade: requirements.txt | $(STAMPS_DIR) $(PREFIX)
	python -m pip install --prefix $(PREFIX) --ignore-installed --upgrade $(PIP_PACKAGES)
	@touch $@

$(STAMPS_DIR)/pipx-install: pipx-requirements.txt | $(STAMPS_DIR) $(PREFIX)
	PIPX_HOME=$(PREFIX)/share/pipx PIPX_BIN_DIR=$(PREFIX)/bin python -m pipx install $(PIPX_PACKAGES)
	PIPX_HOME=$(PREFIX)/share/pipx PIPX_BIN_DIR=$(PREFIX)/bin python -m pipx inject yt-dlp mutagen
	@touch $@

$(STAMPS_DIR)/pipx-upgrade: pipx-requirements.txt | $(STAMPS_DIR) $(PREFIX)
	PIPX_HOME=$(PREFIX)/share/pipx PIPX_BIN_DIR=$(PREFIX)/bin python -m pipx install $(PIPX_PACKAGES)
	PIPX_HOME=$(PREFIX)/share/pipx PIPX_BIN_DIR=$(PREFIX)/bin python -m pipx upgrade-all
	PIPX_HOME=$(PREFIX)/share/pipx PIPX_BIN_DIR=$(PREFIX)/bin python -m pipx inject yt-dlp mutagen --force
	@touch $@

$(STAMPS_DIR)/pyenv-rehash: $(_pip_stamp) $(_pipx_stamp) | $(STAMPS_DIR)
	pyenv rehash
	@touch $@

install:
	$(MAKE) NO_UPGRADE=1

upgrade:
	$(MAKE) NO_UPGRADE=0

$(DEB_PKG): $(SPEC)
	@mkdir -p dist
	@mkdeb

clean:
	@rm -rf $(STAMPS_DIR) $(BUILD_DIR) $(DEB_PKG)
