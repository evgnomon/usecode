COLLECTION_DIR := ansible_collections/evgnomon/catamaran
COLLECTION_VERSION := $(shell awk '/^version:/ {print $$2}' $(COLLECTION_DIR)/galaxy.yml)
COLLECTION_ARTIFACT := $(COLLECTION_DIR)/evgnomon-catamaran-$(COLLECTION_VERSION).tar.gz

.PHONY: all check build build-python build-collection publish publish-pypi publish-galaxy clean

all: build

check:
	uv run poe check

build: build-python build-collection

build-python:
	uv build

build-collection:
	cd $(COLLECTION_DIR) && ansible-galaxy collection build --force

publish: publish-pypi publish-galaxy

publish-pypi: clean build-python
	@UV_PUBLISH_TOKEN=`getsecret | jq -r '.pypi.all'` uv publish

publish-galaxy: build-collection
	@ansible-galaxy collection publish $(COLLECTION_ARTIFACT) --token `getsecret | jq -r '.ansible_galaxy.token'`

clean:
	rm -rf dist $(COLLECTION_DIR)/*.tar.gz
