# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

version := 13.0
ROOT_DIR ?= $(abspath $(CURDIR)/../../../..)

.PHONY: all build dist publish

all: dist

build: debian-13-genericcloud-amd64.img

debian-13-genericcloud-amd64.img:
	@wget https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-amd64.qcow2 -O debian-13-genericcloud-amd64.img

# pkg/debian links the image in as /usr/share/vm/images/debian-trixie,
# where uc-vm-local looks for base images.
dist: build
	@uc deb build -v $(version) -o $(ROOT_DIR)/dist

publish: dist
	@cd $(ROOT_DIR) && uc deb publish
