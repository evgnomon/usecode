#!/bin/sh

set -e

IS_WSL2=$(grep -qi "microsoft" /proc/version 2>/dev/null && echo true || echo false)
PYTHON_VERSION="3.14.3"



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

ansible-playbook -i inventory.py -e ansible_python_interpreter=$HOME/.pyenv/shims/python3 $PLAYARGS main.yaml
