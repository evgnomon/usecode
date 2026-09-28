# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := jsonc
BIN := uc-data-jsonc
# The old name, kept for a release so scripts calling it carry on working.
ALIASES := jsonc

include ../rust.mk

.PHONY: dist publish

dist: build
	@uc-deb-build

publish: dist
	@uc-deb-publish
