# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := mkdeb
BIN := uc-deb-build
# The old name, kept for a release so scripts calling it carry on working.
ALIASES := mkdeb

include ../rust.mk

.PHONY: dist

dist: build
	@install -d pkg/debian/usr/bin
	@install -m 0755 $(OUTPUT) pkg/debian/usr/bin/$(BIN)
	@$(OUTPUT)
