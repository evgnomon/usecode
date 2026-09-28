# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := workd
PKG_BIN := pkg/debian/usr/bin/$(NAME)

include ../rust.mk

.PHONY: dist publish

dist: $(PKG_BIN)
	@mkdeb

$(PKG_BIN): $(OUTPUT)
	@install -d $(@D)
	@install -m 0755 $(OUTPUT) $@

publish: dist
	@pubdeb
