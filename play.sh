#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

set -euo pipefail

export PATH=$HOME/.local/share/mise/shims:$HOME/.local/bin:$HOME/.rbenv/shims:$HOME/.rbenv/bin:$HOME/.local/libexec:$HOME/bin:$HOME/go/bin:$HOME/.cargo/bin:$HOME/.gem/bin:$HOME/.cargo/bin:$HOME/.local/bin:/usr/local/bin:/usr/bin:/bin:/usr/local/games:/usr/games:$HOME/.dotnet/tools

sudo apt update
sudo DEBIAN_FRONTEND=noninteractive apt upgrade -y \
  -o Dpkg::Options::="--force-confold" \
  -o Dpkg::Options::="--force-confdef"
sudo apt install -y git make

mkdir -p ~/src/github.com/evgnomon
cd ~/src/github.com/evgnomon

if [ ! -d "usecode" ]; then
  git clone https://github.com/evgnomon/usecode.git
  cd usecode
  git checkout master
else
  cd usecode
  git pull
  git checkout master
fi

git submodule update --init --recursive

cd lib/configurator
sudo make prepare

sudo apt autoremove -y
make play

cd ../jsonc
make
sudo make install

cd ../python
make
sudo make install
sudo ldconfig

cd ../ppkgs
make
sudo make install

if [[ ! -d "$HOME/.vim" ]]; then
  mkdir $HOME/.vim
fi
cd ../vim
make init

cd ~/src/github.com/evgnomon/usecode

make
sudo make install
make link
