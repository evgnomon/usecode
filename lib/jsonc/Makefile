# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := jsonc

include ../rust.mk

.PHONY: dist publish

dist: build
	@mkdeb

publish: dist
	@pubdeb
