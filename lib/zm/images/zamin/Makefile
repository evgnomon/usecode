prefix := /usr
version := 0.1.0
DESTDIR ?= /
IMAGE_DIR := $(DESTDIR)$(patsubst /%,%,$(prefix))/share/zm/images
INSTALLED_IMAGE := $(IMAGE_DIR)/zamin

all: dist

.PHONY: build build-image clean install uninstall dist publish

build: build-image

dist: dist/zamin_$(version)_amd64.deb

dist/zamin_$(version)_amd64.deb: output-zamin/zamin
	@mkdeb --version $(version)

publish: dist
	@pubdeb

build-image: output-zamin/zamin

install: $(INSTALLED_IMAGE)

$(INSTALLED_IMAGE): output-zamin/zamin
	@install -d $(@D)
	@install -m 0644 $< $@

uninstall:
	@rm -f $(INSTALLED_IMAGE)
	@rmdir $(IMAGE_DIR) 2>/dev/null || true

clean:
	@rm -rf output-zamin

output-zamin/zamin: zamin.pkr.hcl
	@rm -rf output-zamin
	@packer build zamin.pkr.hcl
