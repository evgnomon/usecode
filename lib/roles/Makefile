.PHONY: all build install link unlink publish clean

NAMESPACE := $(shell awk '/^namespace:/ {print $$2}' galaxy.yml)
NAME      := $(shell awk '/^name:/ {print $$2}' galaxy.yml)
VERSION   := $(shell ucversion)
DIST_DIR  := dist
ARTIFACT  := $(DIST_DIR)/$(NAMESPACE)-$(NAME)-$(VERSION).tar.gz
LINK_DIR  := $(HOME)/.ansible/collections/ansible_collections/$(NAMESPACE)
LINK_PATH := $(LINK_DIR)/$(NAME)

all: build

build: $(ARTIFACT)

$(ARTIFACT): galaxy.yml
	mkdir -p $(DIST_DIR)
	sed -i 's/^version: .*/version: $(VERSION)/' galaxy.yml
	ansible-galaxy collection build --force --output-path $(DIST_DIR)

install: $(ARTIFACT)
	ansible-galaxy collection install --force $(ARTIFACT)

link:
	mkdir -p $(LINK_DIR)
	rm -rf $(LINK_PATH)
	ln -s $(CURDIR) $(LINK_PATH)

unlink:
	rm -f $(LINK_PATH)

publish: $(ARTIFACT)
	ansible-galaxy collection publish --token "$$(getsecret | jq -r '.ansible_galaxy.token')" $(ARTIFACT)

clean:
	rm -rf $(DIST_DIR)
