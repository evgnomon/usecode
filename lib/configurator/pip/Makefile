.PHONY: all install upgrade clean

NO_UPGRADE ?= 0

STAMPS_DIR := .stamps
BUILD_DIR  := $(CURDIR)/build

PIP_PACKAGES  := $(shell grep -v '^\s*#' requirements.txt | grep -v '^\s*$$' | tr '\n' ' ')
PIPX_PACKAGES := $(shell grep -v '^\s*#' pipx-requirements.txt | grep -v '^\s*$$' | tr '\n' ' ')

ifeq ($(NO_UPGRADE),1)
_pip_stamp  := $(STAMPS_DIR)/pip-install
_pipx_stamp := $(STAMPS_DIR)/pipx-install
else
_pip_stamp  := $(STAMPS_DIR)/pip-upgrade
_pipx_stamp := $(STAMPS_DIR)/pipx-upgrade
endif

all: $(_pip_stamp) $(_pipx_stamp) $(STAMPS_DIR)/pyenv-rehash

$(STAMPS_DIR):
	@mkdir -p $@

$(BUILD_DIR):
	@mkdir -p $@

$(STAMPS_DIR)/pip-install: requirements.txt | $(STAMPS_DIR) $(BUILD_DIR)
	python -m pip install --prefix $(BUILD_DIR) --ignore-installed $(PIP_PACKAGES)
	@touch $@

$(STAMPS_DIR)/pip-upgrade: requirements.txt | $(STAMPS_DIR) $(BUILD_DIR)
	python -m pip install --prefix $(BUILD_DIR) --ignore-installed --upgrade $(PIP_PACKAGES)
	@touch $@

$(STAMPS_DIR)/pipx-install: pipx-requirements.txt | $(STAMPS_DIR) $(BUILD_DIR)
	PIPX_HOME=$(BUILD_DIR)/share/pipx PIPX_BIN_DIR=$(BUILD_DIR)/bin python -m pipx install $(PIPX_PACKAGES)
	@touch $@

$(STAMPS_DIR)/pipx-upgrade: pipx-requirements.txt | $(STAMPS_DIR) $(BUILD_DIR)
	PIPX_HOME=$(BUILD_DIR)/share/pipx PIPX_BIN_DIR=$(BUILD_DIR)/bin python -m pipx install $(PIPX_PACKAGES)
	PIPX_HOME=$(BUILD_DIR)/share/pipx PIPX_BIN_DIR=$(BUILD_DIR)/bin python -m pipx upgrade-all
	@touch $@

$(STAMPS_DIR)/pyenv-rehash: $(_pip_stamp) $(_pipx_stamp) | $(STAMPS_DIR)
	pyenv rehash
	@touch $@

install:
	$(MAKE) NO_UPGRADE=1

upgrade:
	$(MAKE) NO_UPGRADE=0

clean:
	@rm -rf $(STAMPS_DIR) $(BUILD_DIR)
