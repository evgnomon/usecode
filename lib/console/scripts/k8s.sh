#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# Builds and rolls out the console from the latest push: the commit the
# branch's upstream (origin/master) is at, not the working tree.
#
#   k8s.sh tag                 the upstream's short commit, after a fetch
#   k8s.sh image <local image> builds lib/console at it as <image>:<tag>
#   k8s.sh deploy [registry]   points console/console at <registry>/usecode-console:<tag>
#
# The registry host comes from CONTAINER_REGISTRY (or REGISTRY); without one
# it's the one the registry's Ingress answers on. `make push` puts the image
# there first.
set -euo pipefail

cd "$(dirname "$0")/.."

rev() {
  git fetch --quiet origin
  git rev-parse --short '@{u}'
}

case ${1:-} in
tag)
  rev
  ;;
image)
  image=$2
  tag=$(rev)
  src=$(mktemp -d)
  trap 'rm -rf "$src"' EXIT
  git archive "$tag" -- . | tar -x -C "$src"
  podman build --network=host -t "$image:$tag" -f "$src/Containerfile" "$src"
  ;;
deploy)
  tag=$(rev)
  registry=${2:-${CONTAINER_REGISTRY:-${REGISTRY:-$(kubectl -n registry get ingress registry -o jsonpath='{.spec.rules[0].host}')}}}
  kubectl -n console set image deployment/console "console=$registry/usecode-console:$tag"
  kubectl -n console rollout status deployment/console --timeout=5m
  ;;
*)
  echo "usage: $0 tag | image <local image> | deploy [registry]" >&2
  exit 2
  ;;
esac
