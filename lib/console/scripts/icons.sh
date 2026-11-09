#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Renders the PNG icons of the web app from public/icon.svg, with
# rsvg-convert (librsvg2-bin) and ImageMagick. Run it after changing the
# icon; the PNGs are committed, so the build needs neither.
set -euo pipefail
cd "$(dirname "$0")/../public"

BG='#15181d' # icon.svg's tile

png() { rsvg-convert -w "$1" -h "$1" icon.svg -o "$2"; }

# Full bleed with the mark in the middle 70%, for launchers that cut their
# own shape (maskable) and for iOS, which rounds the corners itself.
bleed() {
  local inner=$(($1 * 7 / 10))
  rsvg-convert -w "$inner" -h "$inner" icon.svg |
    magick -size "$1x$1" "xc:$BG" - -gravity center -composite -strip "$2"
}

png 192 icon-192.png
png 512 icon-512.png
bleed 512 icon-maskable-512.png
bleed 180 apple-touch-icon.png
