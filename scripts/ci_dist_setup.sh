#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Gets a fresh CI runner ready for make dist: the apt build deps, uc,
# uc-deb-build and jsonc on PATH (the lib dists pack with uc deb build, which
# runs uc-deb-build, and lib/python reads its version with jsonc), jsonc in
# /usr/local/bin too (uc-deb-build runs it from there for .deb.jsonc), and
# CPython in /usr/local for vim to link against.
# Run from the repo root.

set -euo pipefail

bin="$HOME/.local/bin"
uc_bin="$PWD/target/x86_64-unknown-linux-musl/${PROFILE:-release}"
export PATH="$bin:$uc_bin:$PATH"
echo "$bin" >>"${GITHUB_PATH:-/dev/null}"
echo "$uc_bin" >>"${GITHUB_PATH:-/dev/null}"

make -C lib/alacritty build-deps
make -C lib/python build-deps
sudo apt-get install -y --no-install-recommends musl-tools jq \
    libx11-dev libxt-dev libsm-dev libice-dev libselinux1-dev \
    libvirt-dev libisoburn-dev libisofs-dev libburn-dev

make -s -C lib/uc build
make -s -C lib/jsonc link
sudo env PATH="$PATH" make -s -C lib/jsonc install
make -s -C lib/mkdeb link

make -s -C lib/python build
sudo env PATH="$PATH" make -s -C lib/python install
sudo ldconfig
