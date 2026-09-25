#!/bin/sh
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Builds uc and configures this machine with `uc configure`. Arguments are
# passed on, e.g. `play.sh -C` for a dry run or `play.sh -t dotfiles,git`.
# The profile comes from INSTALL_PROFILE, else DEV_CONTAINER and WSL.

set -e

. "$HOME/.cargo/env"
cd "$(dirname "$0")/../uc"
make build
exec target/x86_64-unknown-linux-musl/release/uc-configure "$@"
