#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Runs cargo. Unless RELEASE=1 it also picks up faster tools when they're
# installed: sccache caches compiled dependencies across clean builds, and
# mold links much quicker than the default linker.
set -euo pipefail

if [ "${RELEASE:-0}" != 1 ]; then
	if command -v sccache >/dev/null; then
		export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"
	fi
	if command -v mold >/dev/null; then
		export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold"
	fi
fi

exec cargo "$@"
