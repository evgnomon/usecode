# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Picks the cargo profile: release by default, fast (see /Cargo.toml) with
# DEBUG=1. Include it after ROOT_DIR is set.

DEBUG ?= 0
PROFILE_0 := release
PROFILE_1 := fast
PROFILE ?= $(PROFILE_$(DEBUG))
CARGO := $(ROOT_DIR)/scripts/cargo.sh
export DEBUG PROFILE
