# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := hgl
BIN := uc-repo-headers
# The old name, kept for a release so scripts calling it carry on working.
ALIASES := hgl

include ../rust.mk

.PHONY: dist publish

dist: build
	@uc-deb-build

publish: dist
	@uc-deb-publish
