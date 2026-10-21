# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

version := 24.04.4.0
ROOT_DIR ?= $(abspath $(CURDIR)/../../../..)

.PHONY: all build dist publish

all: dist

build: noble-server-cloudimg-amd64.img

noble-server-cloudimg-amd64.img:
	@wget https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img -O noble-server-cloudimg-amd64.img

# pkg/debian links the image in as /usr/share/vm/images/ubuntu-noble,
# where uc-vm-local looks for base images.
dist: build
	@uc deb build -v $(version) -o $(ROOT_DIR)/dist

publish: dist
	@cd $(ROOT_DIR) && uc deb publish
