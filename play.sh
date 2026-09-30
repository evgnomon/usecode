#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Kickstart: clone (or update) usecode, set up this machine and install uc.
# Safe to run again; it picks up where it left off.
#
#   USECODE_DIR      where the checkout lives (default ~/src/github.com/evgnomon/usecode)
#   USECODE_BRANCH   branch to build (default master)
#   INSTALL_PROFILE  workstation, dev_container, wsl or vm (detected when unset)

set -euo pipefail

USECODE_DIR=${USECODE_DIR:-$HOME/src/github.com/evgnomon/usecode}
USECODE_BRANCH=${USECODE_BRANCH:-master}

export PATH=$HOME/.local/share/mise/shims:$HOME/.local/bin:$HOME/.rbenv/shims:$HOME/.rbenv/bin:$HOME/.local/libexec:$HOME/bin:$HOME/go/bin:$HOME/.cargo/bin:$HOME/.gem/bin:$HOME/.dotnet/tools:/usr/local/bin:/usr/bin:/bin:/usr/local/games:/usr/games

step() { printf '\n==> %s\n' "$*"; }

step "Updating apt packages and installing git, make and curl"
sudo apt update
sudo DEBIAN_FRONTEND=noninteractive apt upgrade -y \
  -o Dpkg::Options::="--force-confold" \
  -o Dpkg::Options::="--force-confdef"
sudo apt install -y git make curl ca-certificates

step "Getting usecode into $USECODE_DIR"
[[ -d "$USECODE_DIR/.git" ]] || git clone https://github.com/evgnomon/usecode.git "$USECODE_DIR"
cd "$USECODE_DIR"
git checkout "$USECODE_BRANCH"
git pull --ff-only
git submodule update --init --recursive

step "Preparing the system (apt packages, needs sudo)"
cd lib/configurator
sudo make prepare
sudo apt autoremove -y

step "Configuring this machine with uc configure"
make play

step "Building shared libraries"
cd ../jsonc
make
sudo make install

cd ../python
make
sudo make install
sudo ldconfig

mkdir -p "$HOME/.vim"
cd ../vim
make init

step "Building and installing the uc tools"
cd "$USECODE_DIR"
make RELEASE=1
sudo make install RELEASE=1
make link RELEASE=1

step "All done"
cat <<EOF
usecode lives in $USECODE_DIR and its tools are in /usr/local/bin.
Open a new shell so PATH changes apply, then try:

  uc help
EOF
