#!/usr/bin/env bash
# Builds the uc binaries for a target triple, installing the target's std on
# first use.
#
# The default target is musl, which links everything into a static executable.
# The crate has no C dependencies, so no cross toolchain is needed for it.
set -euo pipefail

profile="${1:?usage: build.sh <profile> <target>}"
target="${2:?usage: build.sh <profile> <target>}"

if ! rustup target list --installed | grep -qx "$target"; then
	echo "installing rust target $target" >&2
	rustup target add "$target"
fi

exec cargo build --profile "$profile" --target "$target"
