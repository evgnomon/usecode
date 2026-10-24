# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# The control-node commands, which go into the uc package, and usecoded,
# which is built and packaged on its own: a static (musl) binary for the
# hosts, released as usecoded-ARCH (what uc daemon install downloads) and
# as the usecoded package.
NAME := uc-daemon
BIN := uc-daemon-ctl
# The control-node tools the same package builds: uc-net-mesh is `uc net
# mesh`, uc-net-firewall is `uc net firewall`, uc-kube is `uc kube`.
EXTRA_BINS := uc-net-mesh uc-net-firewall uc-kube
EXTRA_SOURCES := init/systemd/usecode.service
# usecoded is left out of the control-node build, see below.
CARGO_ARGS := $(addprefix --bin ,$(BIN) $(EXTRA_BINS))

include ../rust.mk

# The machine's own arch; USECODED_TARGET=aarch64-unknown-linux-musl
# cross-builds for arm.
USECODED_TARGET ?= $(shell uname -m)-unknown-linux-musl
USECODED_ARCH := $(firstword $(subst -, ,$(USECODED_TARGET)))
USECODED := $(ROOT_DIR)/target/$(USECODED_TARGET)/$(PROFILE)/usecoded
USECODED_ROOT := $(CURDIR)/pkg/debian
DIST_DIR := $(ROOT_DIR)/dist
# The release uc daemon install downloads usecoded from (see install.rs).
VERSION ?= $(shell git describe --tags --match 'v*' --abbrev=0 2>/dev/null | sed 's/^v//')
export VERSION

.PHONY: usecoded dist

build: $(USECODED)

usecoded: $(USECODED)

$(USECODED): $(SOURCES)
	@$(CARGO) build -p $(NAME) --bin usecoded --profile $(PROFILE) --target $(USECODED_TARGET)
	@touch $@

# usecoded-ARCH for uc daemon install, and the usecoded package for hosts
# that would rather apt install it: usecode.service and the scripts in
# debian/ enable and start it.
dist: $(USECODED)
	@install -Dm755 $(USECODED) $(DIST_DIR)/usecoded-$(USECODED_ARCH)
	@rm -rf $(USECODED_ROOT)
	@install -Dm755 $(USECODED) $(USECODED_ROOT)/usr/local/bin/usecoded
	@install -Dm644 init/systemd/usecode.service $(USECODED_ROOT)/usr/lib/systemd/system/usecode.service
	@install -Dm755 -t $(USECODED_ROOT)/DEBIAN debian/postinst debian/prerm debian/postrm
	@uc deb build -v "$(VERSION)" -o $(DIST_DIR)
