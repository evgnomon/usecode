COLLECTION_DIR := ansible_collections/evgnomon/catamaran
VERSION ?= $(shell git describe --tags --match 'v*' --abbrev=0 2>/dev/null | sed 's/^v//')
COLLECTION_ARTIFACT := $(COLLECTION_DIR)/evgnomon-catamaran-$(VERSION).tar.gz
ROOT_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_DIR ?= $(ROOT_DIR)/build
STAMP_DIR := $(ROOT_DIR)/.stamps/$(patsubst $(ROOT_DIR)/%,%,$(CURDIR))
PYTHON_ARTIFACT := dist/catamaran-$(VERSION).tar.gz
PYPI_STAMP := $(STAMP_DIR)/pypi-$(VERSION)
GALAXY_STAMP := $(STAMP_DIR)/galaxy-$(VERSION)
VERSION_STAMP := $(STAMP_DIR)/version-$(VERSION)

.PHONY: all check build build-python build-collection publish publish-pypi publish-galaxy clean clean-python clean-collection clean-stamps

all: build

check:
	uv run poe check


$(VERSION_STAMP):
	@sed -i 's/^version = ".*"/version = "$(VERSION)"/' pyproject.toml
	@sed -i 's/^version: .*/version: $(VERSION)/' $(COLLECTION_DIR)/galaxy.yml
	@mkdir -p $(STAMP_DIR)
	@touch $@

build: build-python build-collection

build-python: $(PYTHON_ARTIFACT)

$(PYTHON_ARTIFACT): $(VERSION_STAMP)
	uv build

build-collection: $(COLLECTION_ARTIFACT)

$(COLLECTION_ARTIFACT): $(VERSION_STAMP)
	cd $(COLLECTION_DIR) && ansible-galaxy collection build --force

publish: publish-pypi publish-galaxy

publish-pypi: $(PYPI_STAMP)

$(PYPI_STAMP): $(PYTHON_ARTIFACT)
	@./scripts/publish-pypi.sh $(VERSION)
	@mkdir -p $(STAMP_DIR)
	@touch $@

publish-galaxy: $(GALAXY_STAMP)

$(GALAXY_STAMP): $(COLLECTION_ARTIFACT)
	@./scripts/publish-galaxy.sh $(VERSION) $(COLLECTION_ARTIFACT)
	@mkdir -p $(STAMP_DIR)
	@touch $@

clean: clean-python clean-collection clean-stamps

clean-python:
	rm -rf dist

clean-collection:
	rm -rf $(COLLECTION_DIR)/*.tar.gz

clean-stamps:
	rm -rf $(STAMP_DIR)
