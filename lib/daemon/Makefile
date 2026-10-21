# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Only the control-node command is installed here; usecoded reaches the
# hosts through uc daemon install, which builds it, copies it over and
# runs `usecoded setup` there.
NAME := uc-daemon
BIN := uc-daemon-ctl
# The control-node tools the same package builds: uc-net-mesh is `uc net
# mesh`, uc-net-firewall is `uc net firewall`, uc-kube is `uc kube`.
# usecoded is not installed here - uc daemon install copies it to a host.
EXTRA_BINS := uc-net-mesh uc-net-firewall uc-kube
EXTRA_SOURCES := init/systemd/usecode.service

include ../rust.mk

# The usecoded package for hosts that would rather apt install it than
# get it from uc daemon install: the same static (musl) build, with
# usecode.service and the scripts in debian/ that enable and start it.
USECODED_TRIPLE := $(shell uname -m)-unknown-linux-musl
USECODED := $(ROOT_DIR)/target/$(USECODED_TRIPLE)/release/usecoded
USECODED_ROOT := $(CURDIR)/pkg/debian
VERSION ?= $(shell git describe --tags --match 'v*' --abbrev=0 2>/dev/null | sed 's/^v//')

.PHONY: dist

dist:
	@cargo build --release --quiet -p $(NAME) --bin usecoded --target $(USECODED_TRIPLE)
	@rm -rf $(USECODED_ROOT)
	@install -Dm755 $(USECODED) $(USECODED_ROOT)/usr/local/bin/usecoded
	@install -Dm644 init/systemd/usecode.service $(USECODED_ROOT)/usr/lib/systemd/system/usecode.service
	@install -Dm755 -t $(USECODED_ROOT)/DEBIAN debian/postinst debian/prerm debian/postrm
	@uc deb build -v "$(VERSION)" -o $(ROOT_DIR)/dist
