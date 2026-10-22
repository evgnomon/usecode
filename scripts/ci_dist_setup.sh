#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Gets a fresh CI runner ready for make dist: the apt build deps, uc
# and uc-deb-build on PATH (the lib dists pack with uc deb build, which runs
# uc-deb-build), and CPython in /usr/local for vim to link against.
# The build and release workflows both run it, so a dist that would break
# the release breaks the build first. SKIP (as in each_lib.sh) leaves out
# the alacritty and python deps and the CPython build.
# Run from the repo root.

set -euo pipefail

skip=" ${SKIP:-} "
release="${RELEASE:-0}"
profile="${PROFILE:-$([[ "$release" == 1 ]] && echo release || echo fast)}"
bin="$HOME/.local/bin"
uc_bin="$PWD/target/$(uname -m)-unknown-linux-musl/$profile"
export PATH="$bin:$uc_bin:$PATH"
echo "$bin" >>"${GITHUB_PATH:-/dev/null}"
echo "$uc_bin" >>"${GITHUB_PATH:-/dev/null}"

# The arm runner may come without Rust; rust-toolchain.toml picks the
# toolchain and targets once rustup is there.
if ! command -v cargo >/dev/null; then
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
    export PATH="$HOME/.cargo/bin:$PATH"
    echo "$HOME/.cargo/bin" >>"${GITHUB_PATH:-/dev/null}"
    rustup toolchain install
fi

sudo apt-get update
[[ "$skip" == *" alacritty "* ]] || make -C lib/alacritty build-deps
[[ "$skip" == *" python "* ]] || make -C lib/python build-deps
sudo apt-get install -y --no-install-recommends musl-tools jq \
    libx11-dev libxt-dev libsm-dev libice-dev libselinux1-dev \
    libvirt-dev libisoburn-dev libisofs-dev libburn-dev

make -s -C lib/uc build
make -s -C lib/mkdeb link

[[ "$skip" == *" python "* ]] && exit 0
make -s -C lib/python build
sudo env PATH="$PATH" RELEASE="$release" PROFILE="$profile" make -s -C lib/python install
sudo ldconfig
