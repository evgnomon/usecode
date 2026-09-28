#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Build alacritty from the current directory (the source tree) and stage the
# binary, desktop entry, icon, man pages and shell completions under
# $BUILD_DIR$PREFIX, ready for `make install` to copy into place.

set -euo pipefail

BUILD_DIR="${BUILD_DIR:?BUILD_DIR must be set}"
PREFIX="${PREFIX:-/usr/local}"
STAGE="$BUILD_DIR$PREFIX"

export CDPATH=

cargo build --release --locked --quiet
[ target/release/alacritty -nt "$STAGE/bin/alacritty" ] || exit 0
echo "alacritty: staging into $STAGE"

install -Dm644 extra/linux/Alacritty.desktop "$STAGE/share/applications/Alacritty.desktop"
install -Dm644 extra/logo/alacritty-term.svg "$STAGE/share/icons/hicolor/scalable/apps/Alacritty.svg"
install -Dm644 extra/alacritty.info "$STAGE/share/alacritty/alacritty.info"

install -Dm644 extra/completions/alacritty.bash "$STAGE/share/bash-completion/completions/alacritty"
install -Dm644 extra/completions/_alacritty "$STAGE/share/zsh/site-functions/_alacritty"
install -Dm644 extra/completions/alacritty.fish "$STAGE/share/fish/vendor_completions.d/alacritty.fish"

for scd in extra/man/*.scd; do
    page="$(basename "$scd" .scd)"
    dir="$STAGE/share/man/man${page##*.}"
    mkdir -p "$dir"
    scdoc < "$scd" | gzip -9n > "$dir/$page.gz"
done

# Staged last: its timestamp marks a complete stage for the check above.
install -Dm755 target/release/alacritty "$STAGE/bin/alacritty"
