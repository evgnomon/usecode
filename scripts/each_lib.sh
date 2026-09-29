#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Run a make target in every lib/* that defines it, quietly.
# Usage: each_lib.sh <target>
# SKIP lists lib names (space separated) to leave out, e.g. SKIP=alacritty.

set -euo pipefail

target="${1:?target required}"
make="${MAKE:-make}"
skip=" ${SKIP:-} "

for d in lib/*/; do
    d="${d%/}"
    [[ "$skip" == *" ${d#lib/} "* ]] && continue
    "$make" -C "$d" -n "$target" >/dev/null 2>&1 || continue
    "$make" -s --no-print-directory -C "$d" "$target"
done
