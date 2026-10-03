#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# Bring the current kube context (a usecode k3s cluster, Traefik as k3s
# ships it) to the desired state: cert-manager, the nginx test site, DNS
# pointing at the workers, then the TLS certificate.
# Idempotent; safe to re-run on a live cluster.
#
# Needs: kubectl, helm, cf (Cloudflare CLI), dig; INFRA_DOMAIN and
# CLOUDFLARE_API_TOKEN set (~/.bashrc.d/infra.sh, ~/.bashrc.d/cloudflare.sh).
set -euo pipefail
cd "$(dirname "$0")"

DOMAIN=${INFRA_DOMAIN:?source ~/.bashrc.d/infra.sh first}
: "${CLOUDFLARE_API_TOKEN:?source ~/.bashrc.d/cloudflare.sh first}"
echo "==> kube context: $(kubectl config current-context)"

echo "==> cert-manager"
k8s/cert-manager/install.sh

echo "==> nginx"
sed "s/@DOMAIN@/$DOMAIN/g" k8s/nginx/nginx.yaml | kubectl apply -f -

echo "==> Cloudflare DNS for $DOMAIN"
./dns.sh "$DOMAIN"

# Applied last: until the issuer exists cert-manager makes no ACME requests,
# so Let's Encrypt never validates against stale DNS.
echo "==> Let's Encrypt issuer and certificate"
kubectl apply -f k8s/cert-manager/cluster-issuer.yaml
kubectl wait certificate/nginx-tls --for=condition=Ready --timeout=10m

echo "==> check"
curl -fsS -o /dev/null -w "    https://$DOMAIN -> %{http_code}\n" "https://$DOMAIN/"
