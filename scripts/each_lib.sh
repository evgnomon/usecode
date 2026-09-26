#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Run a make target in every lib/* that defines it, quietly.
# Usage: each_lib.sh <target>

set -euo pipefail

target="${1:?target required}"
make="${MAKE:-make}"

for d in lib/*/; do
    d="${d%/}"
    "$make" -C "$d" -n "$target" >/dev/null 2>&1 || continue
    "$make" -s --no-print-directory -C "$d" "$target"
done
