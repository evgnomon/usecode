#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# Ships the API: builds the image at the latest push (origin/master, not the
# working tree), pushes it to the cluster's registry as <sha> and latest,
# rolls console/api out at it and checks https://api.<host>/health answers.
#
#   deploy.sh [push | rollout] [registry host]
#
# `push` only builds and pushes, `rollout` only points console/api at the
# latest push; with neither it does both. console/api is made by
# `uc kube sync -t console`, which a new cluster needs once, after the
# first push. The registry host comes from CONTAINER_REGISTRY (or
# REGISTRY); without one it's the one the registry's Ingress answers on.
set -euo pipefail

cd "$(dirname "$0")/../../.."

step=all
case ${1:-} in push | rollout) step=$1 && shift ;; esac
image=localhost/usecode-api
registry=${1:-${CONTAINER_REGISTRY:-${REGISTRY:-$(kubectl -n registry get ingress registry -o jsonpath='{.spec.rules[0].host}')}}}

git fetch --quiet origin
tag=$(git rev-parse --short '@{u}')

push() {
  # What gets built is the upstream, so say when it isn't what's here.
  ahead=$(git rev-list --count '@{u}..HEAD')
  [ "$ahead" = 0 ] || echo "warning: $ahead local commit(s) not pushed are left out; git push first to ship them" >&2
  git diff --quiet HEAD -- lib/api Cargo.lock || echo "warning: uncommitted changes in lib/api are left out" >&2

  src=$(mktemp -d)
  trap 'rm -rf "$src"' EXIT
  # The workspace's manifests and the crates; the API's Containerfile builds
  # only the API from them.
  git archive "$tag" -- Cargo.toml Cargo.lock rust-toolchain.toml lib | tar -x -C "$src"
  podman build --network=host --build-arg PROFILE=release -t "$image:$tag" -f "$src/lib/api/Containerfile" "$src"

  secret() {
    kubectl -n registry get secret registry-auth -o "jsonpath={.data.$1}" | base64 -d
  }
  secret password | podman login "$registry" -u "$(secret username)" --password-stdin
  for t in "$tag" latest; do
    podman tag "$image:$tag" "$registry/usecode-api:$t"
    podman push "$registry/usecode-api:$t"
  done
}

rollout() {
  if ! host=$(kubectl -n console get ingress api -o jsonpath='{.spec.rules[0].host}' 2>/dev/null); then
    echo "console/api isn't on the cluster yet: run \`uc kube sync -t console\` once, then deploy again" >&2
    exit 1
  fi
  live=$(kubectl -n console get statefulset api -o jsonpath='{.spec.template.spec.containers[0].image}')
  echo "rolling the API out at $tag on https://$host (live now: ${live##*:})"
  kubectl -n console set image statefulset/api "api=$registry/usecode-api:$tag"
  kubectl -n console rollout status statefulset/api --timeout=10m
  for _ in $(seq 30); do
    if curl -fsS -o /dev/null "https://$host/health"; then
      echo "https://$host is up on $tag"
      return
    fi
    sleep 2
  done
  echo "https://$host/health doesn't answer; see kubectl -n console logs statefulset/api" >&2
  exit 1
}

case $step in
push) push ;;
rollout) rollout ;;
all) push && rollout ;;
esac
