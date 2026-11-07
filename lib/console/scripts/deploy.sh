#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# Ships the console: builds the image at the latest push (origin/master, not
# the working tree), pushes it to the cluster's registry, rolls
# console/console out at it and checks the site answers.
#
#   deploy.sh [registry host]
#
# Run it from anywhere, with kubectl pointing at the cluster. The registry
# host comes from CONTAINER_REGISTRY (or REGISTRY); without one it's the one
# the registry's Ingress answers on.
set -euo pipefail

cd "$(dirname "$0")/.."

image=localhost/usecode-console
registry=${1:-${CONTAINER_REGISTRY:-${REGISTRY:-$(kubectl -n registry get ingress registry -o jsonpath='{.spec.rules[0].host}')}}}

git fetch --quiet origin
tag=$(git rev-parse --short '@{u}')

# What gets built is the upstream, so say when it isn't what's here.
ahead=$(git rev-list --count '@{u}..HEAD')
[ "$ahead" = 0 ] || echo "warning: $ahead local commit(s) not pushed are left out; git push first to ship them" >&2
git diff --quiet HEAD -- . || echo "warning: uncommitted changes in lib/console are left out" >&2

live=$(kubectl -n console get deployment console -o jsonpath='{.spec.template.spec.containers[0].image}')
host=$(kubectl -n console get ingress console -o jsonpath='{.spec.rules[0].host}')
echo "deploying $tag to https://$host (live now: ${live##*:})"

./scripts/k8s.sh image "$image"
./scripts/push.sh "$image" "$tag" "$registry"
./scripts/k8s.sh deploy "$registry"

# Quiet while it tries: a 502 or two while the ingress moves to the new pods
# is normal. Only giving up is worth saying.
for _ in $(seq 30); do
    if curl -fs -o /dev/null "https://$host/healthz"; then
        echo "https://$host is up on $tag"
        exit 0
    fi
    sleep 2
done
curl -fsS -o /dev/null "https://$host/healthz" || true
echo "https://$host/healthz doesn't answer; see kubectl -n console logs deploy/console" >&2
exit 1
