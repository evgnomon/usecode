#!/bin/bash
set -euo pipefail  # Recommended: fail on errors, unset variables, or pipeline failures

export CDPATH=
export LDFLAGS="-rdynamic"

./configure \
    --with-features=huge \
    --disable-nls \
    --enable-gtk2-check \
    --enable-gnome-check \
    --with-x \
    --enable-multibyte \
    --enable-pythoninterp=yes \
    --with-python-command="$HOME/.pyenv/shims/python2" \
    --enable-python3interp=yes \
    --with-python3-command="$HOME/.pyenv/shims/python3" \
    --enable-perlinterp=yes \
    --enable-luainterp=yes \
    --enable-cscope \
    --prefix="$HOME/.local" \
    --with-luajit

make install
