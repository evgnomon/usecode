# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Picks the cargo profile: fast (see /Cargo.toml) by default, release with
# RELEASE=1. Include it after ROOT_DIR is set.

RELEASE ?= 0
PROFILE_0 := fast
PROFILE_1 := release
PROFILE ?= $(PROFILE_$(RELEASE))
CARGO := $(ROOT_DIR)/scripts/cargo.sh
export RELEASE PROFILE
