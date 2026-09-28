# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := vm
BIN := uc-vm-local
# The old name, kept for a release so scripts calling it carry on working.
ALIASES := vm
EXTRA_SOURCES := build.rs
PREFIX ?= /usr

include ../rust.mk

.PHONY: dist publish

dist: build
	@uc-deb-build

publish: dist
	@uc-deb-publish
