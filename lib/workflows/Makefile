# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

.PHONY: build install clean

PKGS:= bin fzurls mkdeb workd

build install clean:
	@for pkg in $(PKGS); do \
		[ -f $$pkg/Makefile ] && $(MAKE) -C $$pkg $@ || true; \
	done
