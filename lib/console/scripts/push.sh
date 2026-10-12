#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# Pushes the console image to the cluster's own registry (the `registry`
# role), as <tag> and latest, logging podman in with the login the
# cluster keeps in registry/registry-auth.
#
#   push.sh <local image> <tag> [registry host]
#
# Without a registry host it's the one the registry's Ingress answers on.
set -euo pipefail

image=$1
tag=$2
registry=${3:-$(kubectl -n registry get ingress registry -o jsonpath='{.spec.rules[0].host}')}
secret() {
  kubectl -n registry get secret registry-auth -o "jsonpath={.data.$1}" | base64 -d
}

secret password | podman login "$registry" -u "$(secret username)" --password-stdin
for t in "$tag" latest; do
  podman tag "$image:$tag" "$registry/usecode-console:$t"
  podman push "$registry/usecode-console:$t"
done
