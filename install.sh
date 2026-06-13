#!/usr/bin/env bash
set -euo pipefail

sudo apt update
sudo apt install -y git make

mkdir -p ~/src/github.com/evgnomon
cd ~/src/github.com/evgnomon

git clone https://github.com/evgnomon/usecode.git

cd usecode/lib/configurator
sudo make install
make link

cd ../workflows
sudo make install

source ~/.bashrc
