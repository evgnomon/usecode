#!/usr/bin/env bash
set -euo pipefail

# Pushes container images to the registry deployed by
# deploy/playbooks/registry.yaml (lib/roles/container-registry).
#
# The registry host is a "protected-servers" node (see
# deploy/playbooks/group_vars/protected-servers.yaml) and is only reachable
# through the "shadow" bastion, so this opens a local SSH tunnel to the
# registry's port before logging in and pushing.
#
# Usage: publish.sh <image>[:tag] [<image>[:tag] ...]
#
# Requires an SSH host entry (or DNS-resolvable name) for the bastion,
# matching deploy/playbooks/group_vars/main.yml's firewall_bastion_hosts.

SCRIPT_DIR="$(dirname -- "$(readlink -f -- "${BASH_SOURCE[0]}")")"
PLAYBOOK_DIR="$SCRIPT_DIR/playbooks"

CONTAINER_CLI="${CONTAINER_CLI:-podman}"
BASTION_HOST="${BASTION_HOST:-shadow}"
REGISTRY_HOST="${REGISTRY_HOST:-167.233.79.126}"
REGISTRY_SSH_PORT="${REGISTRY_SSH_PORT:-2657}"
REGISTRY_SSH_USER="${REGISTRY_SSH_USER:-root}"
REGISTRY_PORT="${REGISTRY_PORT:-5000}"
LOCAL_PORT="${LOCAL_PORT:-$REGISTRY_PORT}"
REGISTRY_USERNAME="${REGISTRY_USERNAME:-registry}"
REGISTRY_ADDR="localhost:${LOCAL_PORT}"

usage() {
  echo "Usage: $0 <image>[:tag] [<image>[:tag] ...]" >&2
  exit 1
}

[ "$#" -ge 1 ] || usage

if [ -z "${REGISTRY_PASSWORD:-}" ]; then
  REGISTRY_PASSWORD="$(ansible-vault view "$PLAYBOOK_DIR/vault.yaml" \
    | sed -n 's/^vault_container_registry_password: *"\?\([^"]*\)"\?$/\1/p')"
fi

if [ -z "$REGISTRY_PASSWORD" ]; then
  echo "Could not determine registry password; set REGISTRY_PASSWORD or check $PLAYBOOK_DIR/vault.yaml" >&2
  exit 1
fi

TUNNEL_PID=""
cleanup() {
  if [ -n "$TUNNEL_PID" ]; then
    kill "$TUNNEL_PID" 2>/dev/null || true
    wait "$TUNNEL_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "Opening SSH tunnel to ${REGISTRY_HOST}:${REGISTRY_PORT} via bastion ${BASTION_HOST}..." >&2
ssh -4 -N -o ExitOnForwardFailure=yes \
  -J "$BASTION_HOST" \
  -p "$REGISTRY_SSH_PORT" \
  -L "127.0.0.1:${LOCAL_PORT}:localhost:${REGISTRY_PORT}" \
  "${REGISTRY_SSH_USER}@${REGISTRY_HOST}" &
TUNNEL_PID=$!

for _ in $(seq 1 30); do
  if (exec 3<>"/dev/tcp/127.0.0.1/${LOCAL_PORT}") 2>/dev/null; then
    exec 3>&- 3<&-
    break
  fi
  if ! kill -0 "$TUNNEL_PID" 2>/dev/null; then
    echo "SSH tunnel exited unexpectedly" >&2
    exit 1
  fi
  sleep 1
done

# The registry is plain HTTP (no TLS configured by the container-registry
# role), so podman needs --tls-verify=false to talk to it over the tunnel.
TLS_ARGS=()
if [ "$CONTAINER_CLI" = "podman" ]; then
  TLS_ARGS=(--tls-verify=false)
fi

echo "$REGISTRY_PASSWORD" | "$CONTAINER_CLI" login "${TLS_ARGS[@]}" "$REGISTRY_ADDR" -u "$REGISTRY_USERNAME" --password-stdin

for IMAGE in "$@"; do
  TARGET="${REGISTRY_ADDR}/${IMAGE}"
  echo "Tagging ${IMAGE} -> ${TARGET}" >&2
  "$CONTAINER_CLI" tag "$IMAGE" "$TARGET"
  echo "Pushing ${TARGET}" >&2
  "$CONTAINER_CLI" push "${TLS_ARGS[@]}" "$TARGET"
done
