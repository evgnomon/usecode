VERSION ?= $(shell git describe --tags --match 'v*' --abbrev=0 2>/dev/null | sed 's/^v//')
ROOT_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_DIR ?= $(ROOT_DIR)/build
STAMP_DIR := $(CURDIR)/.stamps
PYTHON_ARTIFACT := dist/usecode-$(VERSION).tar.gz
PYPI_STAMP := $(STAMP_DIR)/pypi-$(VERSION)
VERSION_STAMP := $(STAMP_DIR)/version-$(VERSION)

.PHONY: all check build build-python install uninstall publish publish-pypi publish-galaxy clean clean-python clean-stamps

all: build

check:
	uv run poe check


$(VERSION_STAMP):
	@sed -i 's/^version = ".*"/version = "$(VERSION)"/' pyproject.toml
	@mkdir -p $(STAMP_DIR)
	@touch $@

build: build-python

build-python: $(PYTHON_ARTIFACT)

$(PYTHON_ARTIFACT): $(VERSION_STAMP)
	uv build

install: $(PYTHON_ARTIFACT)
	uv tool install --reinstall $(PYTHON_ARTIFACT)

uninstall:
	uv tool uninstall usecode

publish: publish-pypi

publish-pypi: $(PYPI_STAMP)

$(PYPI_STAMP): $(PYTHON_ARTIFACT)
	@./scripts/publish-pypi.sh $(VERSION)
	@mkdir -p $(STAMP_DIR)
	@touch $@

clean: clean-python clean-stamps

clean-python:
	rm -rf dist

clean-stamps:
	rm -rf $(STAMP_DIR)
