#!/usr/bin/env bash
set -euo pipefail

sudo apt update
sudo apt install -y git make

mkdir -p ~/src/github.com/evgnomon
cd ~/src/github.com/evgnomon

if [ ! -d "usecode" ]; then
  git clone https://github.com/evgnomon/usecode.git
  git checkout ubuntu
fi

cd usecode/lib/configurator
sudo make prepare
make play

cd ../workflows
sudo make install

source ~/.bashrc
