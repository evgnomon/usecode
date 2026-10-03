#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# Point DOMAIN at the cluster's workers: one A record per worker's public
# IPv4, nothing else. Stale A records are removed, and so are AAAA ones,
# since Traefik's load balancer only listens on IPv4. Then wait until
# public resolvers agree.
#
# Usage: dns.sh [DOMAIN]   (default $INFRA_DOMAIN)
# Needs: kubectl, cf (Cloudflare CLI), dig, python3; CLOUDFLARE_API_TOKEN set.
set -euo pipefail

DOMAIN=${1:-${INFRA_DOMAIN:?pass a domain or source ~/.bashrc.d/infra.sh}}
: "${CLOUDFLARE_API_TOKEN:?source ~/.bashrc.d/cloudflare.sh first}"

# k3s reports the public addresses as InternalIP; keep the IPv4 ones.
want=$(kubectl get nodes -l '!node-role.kubernetes.io/control-plane' \
  -o jsonpath='{range .items[*].status.addresses[?(@.type=="InternalIP")]}{.address}{"\n"}{end}' \
  | grep -v : | sort -u)
[ -n "$want" ] || { echo "no worker nodes found" >&2; exit 1; }

records() { # type -> "id content" lines
  cf dns records list -z "$DOMAIN" --name "$DOMAIN" --type "$1" \
    | python3 -c 'import sys,json; [print(r["id"], r["content"]) for r in json.load(sys.stdin)]'
}

while read -r id content; do
  [ -n "$id" ] || continue
  if grep -qxF "$content" <<<"$want"; then continue; fi
  cf dns records delete "$id" -z "$DOMAIN" -q --force >/dev/null
  echo "    removed A $DOMAIN -> $content"
done <<<"$(records A)"

while read -r id content; do
  [ -n "$id" ] || continue
  cf dns records delete "$id" -z "$DOMAIN" -q --force >/dev/null
  echo "    removed AAAA $DOMAIN -> $content"
done <<<"$(records AAAA)"

have=$(records A | cut -d' ' -f2)
for ip in $want; do
  if grep -qxF "$ip" <<<"$have"; then echo "    A $DOMAIN -> $ip"; continue; fi
  body="{\"type\":\"A\",\"name\":\"$DOMAIN\",\"content\":\"$ip\",\"proxied\":false,\"ttl\":60}"
  cf dns records create -z "$DOMAIN" -q --body "$body" >/dev/null
  echo "    A $DOMAIN -> $ip (new)"
done

echo "==> waiting for public DNS to return $(tr '\n' ' ' <<<"$want")"
for _ in $(seq 60); do
  [ "$(dig +short "$DOMAIN" A @1.1.1.1 | sort)" = "$want" ] \
    && [ "$(dig +short "$DOMAIN" A @8.8.8.8 | sort)" = "$want" ] && exit 0
  sleep 10
done
echo "public DNS still disagrees after 10 minutes" >&2
exit 1
