#!/usr/bin/env bash
set -euo pipefail

mkdir -p ~/src/github.com/evgnomon
cd ~/src/github.com/evgnomon

git clone git@github.com:evgnomon/usecode.git

cd usecode/lib/configurator
sudo make install
make link

cd ../workflows
sudo make install

source ~/.bashrc
