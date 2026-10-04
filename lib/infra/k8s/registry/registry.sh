#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# Install the registry on HOST. The login (user "usecode", a random
# password) is created once and kept in the registry-auth secret; re-runs
# leave it alone.
#
# Usage: registry.sh HOST
set -euo pipefail
cd "$(dirname "$0")"

HOST=${1:?usage: registry.sh HOST}

kubectl create namespace registry --dry-run=client -o yaml | kubectl apply -f -
if ! kubectl -n registry get secret registry-auth >/dev/null 2>&1; then
  kubectl -n registry create secret generic registry-auth \
    --from-literal=username=usecode \
    --from-literal=password="$(openssl rand -hex 24)" \
    --from-literal=http-secret="$(openssl rand -hex 32)"
  echo "    new login in secret registry/registry-auth"
fi
sed "s/@HOST@/$HOST/g" registry.yaml | kubectl apply -f -
kubectl -n registry rollout status deploy/registry --timeout 5m
