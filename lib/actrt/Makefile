COLLECTION_DIR := ansible_collections/evgnomon/catamaran
VERSION ?= $(shell awk '/^VERSION[[:space:]]*:=/ {print $$3}' ../../Makefile)
COLLECTION_ARTIFACT := $(COLLECTION_DIR)/evgnomon-catamaran-$(VERSION).tar.gz
STAMP_DIR := .stamps
PYTHON_ARTIFACT := dist/catamaran-$(VERSION).tar.gz
PYPI_STAMP := $(STAMP_DIR)/pypi-$(VERSION)
GALAXY_STAMP := $(STAMP_DIR)/galaxy-$(VERSION)
VERSION_STAMP := $(STAMP_DIR)/version-$(VERSION)

.PHONY: all check build build-python build-collection publish publish-pypi publish-galaxy clean

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
	@UV_PUBLISH_TOKEN=`getsecret | jq -r '.pypi.all'` uv publish dist/catamaran-$(VERSION)-py3-none-any.whl dist/catamaran-$(VERSION).tar.gz
	@mkdir -p $(STAMP_DIR)
	@touch $@

publish-galaxy: $(GALAXY_STAMP)

$(GALAXY_STAMP): $(COLLECTION_ARTIFACT)
	@ansible-galaxy collection publish $(COLLECTION_ARTIFACT) --token `getsecret | jq -r '.ansible_galaxy.token'`
	@mkdir -p $(STAMP_DIR)
	@touch $@

clean:
	rm -rf dist $(COLLECTION_DIR)/*.tar.gz
