#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
IMAGE=${USECODE_IMAGE:-usecode:dev}
RUNTIME=${CONTAINER_RUNTIME:-}
CONTAINERFILE=${2:-.devcontainer/Containerfile}
RUNTIME_OPTIONS=()
BUILD_OPTIONS=()
RUNTIME_COMMAND=()
PODMAN_ROOTFUL=${PODMAN_ROOTFUL:-auto}

usage() {
	cat <<'EOF'
Usage: scripts/containerize.sh [build|run] [container-file]

Build or start a usecode container. The optional container-file argument applies to build.

Environment variables:
  CONTAINER_RUNTIME  Container CLI to use: podman or docker
  USECODE_IMAGE      Image tag (default: usecode:dev)
	container-file     Containerfile or Dockerfile, relative to the repository root
	PODMAN_ROOTFUL     Podman mode: auto, 1, or 0 (default: auto)
EOF
}

if [[ -z "$RUNTIME" ]]; then
	if command -v podman >/dev/null 2>&1; then
		RUNTIME=podman
	elif command -v docker >/dev/null 2>&1; then
		RUNTIME=docker
	else
		echo "No supported container runtime found; install Podman or Docker." >&2
		exit 1
	fi
fi

if ! command -v "$RUNTIME" >/dev/null 2>&1; then
	echo "Container runtime '$RUNTIME' is not available on PATH." >&2
	exit 1
fi

if [[ "$CONTAINERFILE" != /* ]]; then
	CONTAINERFILE="$ROOT_DIR/$CONTAINERFILE"
fi

if [[ ! -f "$CONTAINERFILE" ]]; then
	echo "Container file '$CONTAINERFILE' does not exist." >&2
	exit 2
fi

if [[ "$RUNTIME" == podman ]]; then
	if [[ "$EUID" -ne 0 && "$PODMAN_ROOTFUL" != 0 ]] \
		&& [[ "$PODMAN_ROOTFUL" == 1 || -e /.containerenv || -e /.dockerenv ]]; then
		if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
			RUNTIME_COMMAND=(sudo -n podman)
		else
			cat >&2 <<'EOF'
Nested Podman needs rootful execution in this Dev Container, but passwordless sudo is unavailable.
Run with a root shell, configure passwordless sudo, or set PODMAN_ROOTFUL=0 to use rootless mode.
EOF
			exit 125
		fi
	else
		RUNTIME_COMMAND=(podman)
	fi
fi

if [[ "$RUNTIME" == podman ]] && ! command -v nft >/dev/null 2>&1; then
	cat >&2 <<'EOF'
Podman networking requires the nft command, which is missing from this image.
Rebuild the Dev Container so the nftables package in .devcontainer/Containerfile is installed.
EOF
	exit 125
fi

if [[ "$RUNTIME" == podman && "$EUID" -ne 0 && "$PODMAN_ROOTFUL" == 0 ]] \
	&& ! command -v newuidmap >/dev/null 2>&1; then
	cat >&2 <<'EOF'
Rootless Podman requires newuidmap/newgidmap, which are missing from this image.
Rebuild the Dev Container so the uidmap package in .devcontainer/Containerfile is installed.
EOF
	exit 125
fi

if [[ "$RUNTIME" == podman && "$EUID" -ne 0 && "$PODMAN_ROOTFUL" == 0 && -r /proc/sys/kernel/apparmor_restrict_unprivileged_userns ]] \
	&& [[ "$(< /proc/sys/kernel/apparmor_restrict_unprivileged_userns)" == 1 ]]; then
	cat >&2 <<'EOF'
Rootless Podman is blocked by AppArmor's unprivileged user-namespace restriction.
Use Docker, run Podman as root, or ask an administrator to enable rootless containers:

  sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0

You can select another runtime with CONTAINER_RUNTIME=docker or CONTAINER_RUNTIME=podman.
EOF
	exit 125
fi

if [[ "$RUNTIME" == podman ]]; then
	RUNTIME_OPTIONS=(--storage-driver=vfs)
	BUILD_OPTIONS=(--network=host)

elif [[ "$RUNTIME" == docker ]]; then
	RUNTIME_COMMAND=(docker)
fi

invoke_runtime() {
	set +e
	"$@"
	local status=$?
	set -e
	if [[ "$RUNTIME" == podman && "$status" -eq 125 ]]; then
		cat >&2 <<'EOF'

	Podman failed while running inside the development container. Nested Podman uses the vfs
	storage driver and host networking for builds; check the preceding error for the cause.
EOF
	fi
	return "$status"
}

build() {
	invoke_runtime "${RUNTIME_COMMAND[@]}" "${RUNTIME_OPTIONS[@]}" build "${BUILD_OPTIONS[@]}" --tag "$IMAGE" --file "$CONTAINERFILE" "$ROOT_DIR"
}

run() {
	invoke_runtime "${RUNTIME_COMMAND[@]}" "${RUNTIME_OPTIONS[@]}" run --rm --interactive --tty \
		--volume "$ROOT_DIR:/workspaces/usecode" \
		--workdir /workspaces/usecode \
		"$IMAGE"
}

case "${1:-run}" in
	build)
		build
	;;
	run)
		run
	;;
	*)
		usage >&2
		exit 2
	;;
esac