#!/bin/sh

set -e

IS_WSL2=$(grep -qi "microsoft" /proc/version 2>/dev/null && echo true || echo false)
PYTHON_VERSION="3.14.3"

if [ ! -d ~/.pyenv ]; then
	git clone https://github.com/pyenv/pyenv.git ~/.pyenv
	echo 'export PYENV_ROOT="$HOME/.pyenv"' >> ~/.bashrc
	echo 'command -v pyenv >/dev/null || export PATH="$PYENV_ROOT/bin:$PATH"' >> ~/.bashrc
	echo 'eval "$(pyenv init -)"' >> ~/.bashrc
else
  git -C ~/.pyenv pull
fi

export PYENV_ROOT="$HOME/.pyenv"
export PATH="$PYENV_ROOT/bin:$PATH"
eval "$(pyenv init --path)"
export PATH="$HOME/.local/bin:$HOME/go/bin:$PATH"

[ ! -d ~/.pyenv/versions/2.7.18 ] && PYTHON_CONFIGURE_OPTS="--enable-shared" pyenv install 2.7.18
[ ! -d ~/.pyenv/versions/$PYTHON_VERSION ] && PYTHON_CONFIGURE_OPTS="--enable-shared" pyenv install $PYTHON_VERSION

pyenv global $PYTHON_VERSION 2.7.18
pip install --upgrade pip
pip install --upgrade ansible pyyaml

which python

[ ! -f  ~/.cargo/bin/rustc ] && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

eval $(cat "$HOME/.cargo/env")
[ ! -d $HOME/.local/bin ] && mkdir -p $HOME/.local/bin

if [ ! -d $HOME/src/github.com/evgnomon ]; then
        mkdir -p $HOME/src/github.com/evgnomon
fi

cd $HOME/src/github.com/evgnomon

if [ ! -d $HOME/src/github.com/evgnomon/usecode ]; then
  git clone https://github.com/evgnomon/usecode.git
fi

cd $HOME/src/github.com/evgnomon/usecode/lib/configurator

PLAYARGS="-e python_version=$PYTHON_VERSION"

if [ "$IS_WSL2" = "true" ]; then
  PLAYARGS="$PLAYARGS -e wsl2=true"
fi

if [ ! -z "$DEV_CONTAINER" ]; then
  PLAYARGS="$PLAYARGS -e dev_container=true"
fi

if [ ! -z "$ASK_BECOME_PASS" ]; then
  PLAYARGS="$PLAYARGS --ask-become-pass"
fi

# ansible-playbook -i inventory.py -e ansible_python_interpreter=$HOME/.pyenv/shims/python3 $PLAYARGS main.yaml
