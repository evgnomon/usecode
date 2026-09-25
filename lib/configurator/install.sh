#!/bin/sh
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Installs the Rust toolchain that builds `uc configure`.

set -e

if [ ! -f ~/.cargo/bin/rustc ]; then
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi

mkdir -p "$HOME/.local/bin"
