# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := mkdeb
BIN := uc-deb-build
# The old name, kept for a release so scripts calling it carry on working.
ALIASES := mkdeb

include ../rust.mk

# The man page goes into the uc deb next to the binary.
deb-stage::
	@cp -r pkg/debian/usr/share $(DEB_ROOT)/usr/
