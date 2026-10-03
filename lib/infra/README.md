<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# infra

What runs on top of a usecode k3s cluster (`uc kube`): cert-manager for
Let's Encrypt certificates, and a small nginx site on your domain so you
can see the whole path work end to end. Traffic comes in through the
Traefik that k3s ships with; nothing about k3s is changed.

```
k8s/cert-manager/   chart values and the Let's Encrypt ClusterIssuer
k8s/nginx/          nginx Deployment, Service and Ingress (HTTPS, HTTP redirected)
dns.sh              points the domain's A records at the workers
bootstrap.sh        installs everything and updates DNS
```

Tools: `kubectl`, `helm`, `cf` (Cloudflare CLI, mise `npm:cf`), `dig`.
Your settings live in `~/.bashrc.d/infra.sh`, credentials in
`~/.bashrc.d/cloudflare.sh`:

```bash
# ~/.bashrc.d/infra.sh
export INFRA_DOMAIN=example.com   # a zone on your Cloudflare account
```

## Bring it up

With your kube context on the cluster:

```bash
source ~/.bashrc.d/infra.sh
source ~/.bashrc.d/cloudflare.sh
lib/infra/bootstrap.sh
```

It installs cert-manager and nginx, points `$INFRA_DOMAIN` at the workers'
public IPv4 addresses, waits until public DNS agrees, and only then adds the
ClusterIssuer. Until the issuer exists cert-manager asks Let's Encrypt for
nothing, so it never validates against stale DNS.

Running it again changes nothing. Added or replaced a worker? Just
`lib/infra/dns.sh` to fix the records.

## Good to know

- DNS lists the workers only. Traefik still answers on every node, control
  planes included, so keep that in mind for the firewall.
- Traefik's load balancer listens on IPv4 only, so `dns.sh` removes AAAA
  records for the domain.
- Let's Encrypt allows 5 certificates per exact set of names per week; avoid
  rebuilding more often than that.
