# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Only the control-node command is installed here; usecoded reaches the
# hosts through uc daemon install, which builds it, copies it over and
# runs `usecoded setup` there.
NAME := uc-daemon
BIN := uc-daemon-ctl
EXTRA_SOURCES := init/systemd/usecode.service

include ../rust.mk
