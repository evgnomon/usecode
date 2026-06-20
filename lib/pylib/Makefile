VERSION ?= $(shell git describe --tags --match 'v*' --abbrev=0 2>/dev/null | sed 's/^v//')
ROOT_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_DIR ?= $(ROOT_DIR)/build
DIST_DIR  := dist
STAMP_DIR := .stamps
PYTHON_ARTIFACT := $(DIST_DIR)/usecode-$(VERSION).tar.gz
PYPI_STAMP := $(STAMP_DIR)/usecode-$(VERSION).published
VERSION_STAMP := $(STAMP_DIR)/usecode-$(VERSION).version

.PHONY: all check build build-python publish publish-pypi clean link unlink

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

publish: publish-pypi

publish-pypi: $(PYPI_STAMP)

$(PYPI_STAMP): $(PYTHON_ARTIFACT)
	./scripts/publish-pypi.sh $(VERSION)
	@mkdir -p $(STAMP_DIR)
	touch $@

link:
	pip install --user -e .

unlink:
	pip uninstall -y usecode

clean:
	rm -rf $(DIST_DIR) $(STAMP_DIR)
