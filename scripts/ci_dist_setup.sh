#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Gets a fresh CI runner ready for make dist: the apt build deps, uc,
# uc-deb-build and jsonc on PATH (the lib dists pack with uc deb build, which
# runs uc-deb-build, and lib/python reads its version with jsonc), jsonc in
# /usr/local/bin too (uc-deb-build runs it from there for .deb.jsonc), and
# CPython in /usr/local for vim to link against.
# The build and release workflows both run it, so a dist that would break
# the release breaks the build first. SKIP (as in each_lib.sh) leaves out
# the alacritty and python deps and the CPython build.
# Run from the repo root.

set -euo pipefail

skip=" ${SKIP:-} "
release="${RELEASE:-0}"
profile="${PROFILE:-$([[ "$release" == 1 ]] && echo release || echo fast)}"
bin="$HOME/.local/bin"
uc_bin="$PWD/target/x86_64-unknown-linux-musl/$profile"
export PATH="$bin:$uc_bin:$PATH"
echo "$bin" >>"${GITHUB_PATH:-/dev/null}"
echo "$uc_bin" >>"${GITHUB_PATH:-/dev/null}"

sudo apt-get update
[[ "$skip" == *" alacritty "* ]] || make -C lib/alacritty build-deps
[[ "$skip" == *" python "* ]] || make -C lib/python build-deps
sudo apt-get install -y --no-install-recommends musl-tools jq \
    libx11-dev libxt-dev libsm-dev libice-dev libselinux1-dev \
    libvirt-dev libisoburn-dev libisofs-dev libburn-dev

make -s -C lib/uc build
make -s -C lib/jsonc link
sudo env PATH="$PATH" RELEASE="$release" PROFILE="$profile" make -s -C lib/jsonc install
make -s -C lib/mkdeb link

[[ "$skip" == *" python "* ]] && exit 0
make -s -C lib/python build
sudo env PATH="$PATH" RELEASE="$release" PROFILE="$profile" make -s -C lib/python install
sudo ldconfig
