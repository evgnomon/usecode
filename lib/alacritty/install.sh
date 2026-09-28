#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# System-side registration after the staged files are copied into $PREFIX:
# compile the terminfo entries and refresh the desktop and icon caches.

set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"

# ncurses does not look under /usr/local, so let tic pick its default
# location (/usr/share/terminfo as root, ~/.terminfo otherwise).
tic -xe alacritty,alacritty-direct "$PREFIX/share/alacritty/alacritty.info"

if command -v update-desktop-database >/dev/null; then
    update-desktop-database -q "$PREFIX/share/applications"
fi
if command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -qtf "$PREFIX/share/icons/hicolor" || true
fi
if command -v mandb >/dev/null; then
    mandb -q "$PREFIX/share/man" 2>/dev/null || true
fi
