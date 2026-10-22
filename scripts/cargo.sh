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
	# mold goes in through cc, which only the x86_64 builds link with; the
	# arm build links with rust-lld (see .cargo/config.toml).
	if command -v mold >/dev/null; then
		for triple in X86_64_UNKNOWN_LINUX_GNU X86_64_UNKNOWN_LINUX_MUSL; do
			var="CARGO_TARGET_${triple}_RUSTFLAGS"
			export "$var=${!var:-} -C link-arg=-fuse-ld=mold"
		done
	fi
fi

exec cargo "$@"
