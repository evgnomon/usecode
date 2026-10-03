#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# Install cert-manager. The ClusterIssuer is applied by bootstrap.sh only
# after DNS points at the cluster, so no ACME order fails early.
set -euo pipefail
cd "$(dirname "$0")"

helm upgrade --install cert-manager oci://quay.io/jetstack/charts/cert-manager \
  --version v1.21.2 \
  -n cert-manager --create-namespace \
  -f values.yaml --wait --timeout 5m
