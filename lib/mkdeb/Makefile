# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := mkdeb

include ../rust.mk

.PHONY: dist

dist: build
	@install -d pkg/debian/usr/bin
	@install -m 0755 $(OUTPUT) pkg/debian/usr/bin/$(NAME)
	@$(OUTPUT)
