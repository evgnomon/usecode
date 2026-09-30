# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Only the control-node command is installed here; usecoded reaches the
# hosts through uc daemon install, which builds it, copies it over and
# runs `usecoded setup` there.
NAME := uc-daemon
BIN := uc-daemon-ctl
# The control-node tools the same package builds: uc-net-mesh is `uc net
# mesh`, uc-net-firewall is `uc net firewall`. usecoded is not installed
# here - uc daemon install copies it to a host.
EXTRA_BINS := uc-net-mesh uc-net-firewall
EXTRA_SOURCES := init/systemd/usecode.service

include ../rust.mk
