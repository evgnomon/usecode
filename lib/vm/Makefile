# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := vm
EXTRA_SOURCES := build.rs
PREFIX ?= /usr

include ../rust.mk

.PHONY: dist publish

dist: build
	@mkdeb

publish: dist
	@pubdeb
