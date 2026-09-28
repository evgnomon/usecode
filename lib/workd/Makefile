# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := workd
BIN := uc-work
# The old name, kept for a release so scripts calling it carry on working.
ALIASES := workd
PKG_BIN := pkg/debian/usr/bin/$(BIN)

include ../rust.mk

.PHONY: dist publish

dist: $(PKG_BIN)
	@uc-deb-build

$(PKG_BIN): $(OUTPUT)
	@install -d $(@D)
	@install -m 0755 $(OUTPUT) $@

publish: dist
	@uc-deb-publish
