#!/usr/bin/env bash
set -euo pipefail

export PATH=/home/$USER/.local/bin:/home/$USER/.rbenv/shims:/home/$USER/.rbenv/bin:/home/$USER/.local/libexec:/home/$USER/bin:/home/$USER/go/bin:/home/$USER/.cargo/bin:/home/$USER/.gem/bin:/home/$USER/.local/bin:/usr/bin:/usr/local/bin

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

cd ../workflows
make
sudo make install PREFIX=/usr/local

cd ../python
make
sudo make install
sudo ldconfig

cd ../ppkgs
make
sudo make install

if [[ ! -d "/home/$USER/.vim" ]]; then
  mkdir /home/$USER/.vim
fi
cd ../vim
make init

cd ~/src/github.com/evgnomon/usecode

make
sudo make install
make link
