#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Run the local stack from deploy/compose.yml.
# Usage: dev.sh up|reload|down|logs [service]
#
#   up      start the stack, building images only if they don't exist yet
#   reload  rebuild every image and recreate every container (after code changes)
#   down    stop and remove the containers (volumes are kept)
#   logs    follow the logs of every service, or of one

set -euo pipefail

cd "$(dirname "$0")/.."
compose=(podman-compose -f deploy/compose.yml)
endpoints=(http://localhost:8430/api http://localhost:8431/api)

die() { echo "error: $*" >&2; exit 1; }

preflight() {
    command -v podman >/dev/null || die "podman is not installed"
    command -v podman-compose >/dev/null || die "podman-compose is not installed (pipx install podman-compose)"
    local sock="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/podman/podman.sock"
    [[ -S "$sock" ]] || die "no podman socket at $sock; run: systemctl --user enable --now podman.socket"
}

# lib/api/.env is optional, but a fresh one gets its own secret key so
# stored provider credentials aren't encrypted with the built-in default.
ensure_env() {
    [[ -f lib/api/.env ]] && return
    cp lib/api/.env.example lib/api/.env
    echo "USECODE_AGENT_SECRET_KEY=$(head -c 32 /dev/urandom | base64 | tr -d '/+=')" >> lib/api/.env
    echo "created lib/api/.env"
}

wait_healthy() {
    local url deadline=$((SECONDS + 120))
    for url in "${endpoints[@]}"; do
        until curl -fsS -o /dev/null "$url/health"; do
            (( SECONDS < deadline )) || die "$url/health not reachable after 120s; see: make logs"
            sleep 2
        done
        echo "ok  $url"
    done
}

case "${1:-}" in
    up)     preflight; ensure_env; "${compose[@]}" up -d; wait_healthy ;;
    reload) preflight; ensure_env; "${compose[@]}" up -d --build --force-recreate; wait_healthy ;;
    down)   "${compose[@]}" down ;;
    logs)   "${compose[@]}" logs -f "${@:2}" ;;
    *)      die "usage: $0 up|reload|down|logs [service]" ;;
esac
