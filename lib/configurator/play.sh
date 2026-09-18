#!/bin/bash

set -e -x

IS_WSL2=$(grep -qi "microsoft" /proc/version 2>/dev/null && echo true || echo false)
PYTHON_VERSION="3.14.3"



PLAYARGS="-e python_version=$PYTHON_VERSION"

if [ -n "${INSTALL_PROFILE:-}" ]; then
  PLAYARGS="$PLAYARGS -e install_profile=$INSTALL_PROFILE"
elif [ -n "${DEV_CONTAINER:-}" ]; then
  PLAYARGS="$PLAYARGS -e install_profile=dev_container"
elif [ "$IS_WSL2" = "true" ]; then
  PLAYARGS="$PLAYARGS -e install_profile=wsl"
fi

if [ ! -z "$ASK_BECOME_PASS" ]; then
  PLAYARGS="$PLAYARGS --ask-become-pass"
fi

export PYENV_ROOT="$HOME/.pyenv"
export PATH="$PYENV_ROOT/bin:$PATH"
eval "$(pyenv init -)"
. "$HOME/.cargo/env"

ansible-playbook -i inventory.py -e ansible_python_interpreter=$HOME/.pyenv/shims/python3 $PLAYARGS main.yaml --ask-become-pass
