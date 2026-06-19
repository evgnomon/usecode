#!/usr/bin/env bash
set -euo pipefail

sudo apt update
sudo apt upgrade -y
sudo apt install -y git make

mkdir -p ~/src/github.com/evgnomon
cd ~/src/github.com/evgnomon

if [ ! -d "usecode" ]; then
  git clone https://github.com/evgnomon/usecode.git
  cd usecode
  git checkout ubuntu
else
  cd usecode
  git pull
  git checkout ubuntu
fi

git submodule update --init --recursive

cd lib/configurator
sudo make prepare

source ~/.bashrc
make play

cd ../workflows
make
sudo make install PREFIX=/usr/local

cd ../python
make
sudo make install

source ~/.bashrc
cd ~/src/github.com/evgnomon/usecode

make
sudo make install
make link
