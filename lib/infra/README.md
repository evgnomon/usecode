<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# infra

What runs on top of a usecode k3s cluster (`uc kube`): cert-manager for
Let's Encrypt certificates, a small nginx site on your domain so you
can see the whole path work end to end. Traffic comes in through the
Traefik that k3s ships with; nothing about k3s is changed. Your private
container registry is one more step on top: `uc kube configure -t registry`
(below).

```
k8s/cert-manager/   chart values and the Let's Encrypt ClusterIssuer
k8s/nginx/          nginx Deployment, Service and Ingress (HTTPS, HTTP redirected)
dns.sh              points a name's A records at the workers
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

It installs cert-manager, points `$INFRA_DOMAIN` at the workers' public
IPv4 addresses, installs nginx, waits until public DNS agrees, and only then
adds the ClusterIssuer. Until the issuer exists cert-manager asks Let's Encrypt for
nothing, so it never validates against stale DNS.

Running it again changes nothing. Added or replaced a worker? Just
`lib/infra/dns.sh` to fix the records.

## Your registry

A private registry for your own images, with its images on a 10Gi Hetzner
Cloud Volume, so it can move to another worker and take them along:

```bash
source ~/.bashrc.d/cloudflare.sh
uc kube configure -t registry -e registry_host=registry.$INFRA_DOMAIN \
  -e registry_dns_target=$INFRA_DOMAIN
```

`registry_dns_target` makes the name a CNAME to your domain, so it follows
the workers by itself. Want more room? `-e registry_size=50Gi` grows the
volume (it never shrinks). Put the variables in your user config and plain
`uc kube configure` keeps it all in shape.

The first run makes one login, user `usecode` with a random password, and
keeps it in a secret. Log in from your machine:

```bash
kubectl -n registry get secret registry-auth -o jsonpath='{.data.password}' \
  | base64 -d | docker login registry.$INFRA_DOMAIN -u usecode --password-stdin
docker tag myapp registry.$INFRA_DOMAIN/myapp:1
docker push registry.$INFRA_DOMAIN/myapp:1
```

To pull from it inside the cluster, give the namespace a pull secret:

```bash
kubectl -n myns create secret docker-registry registry \
  --docker-server=registry.$INFRA_DOMAIN --docker-username=usecode \
  --docker-password="$(kubectl -n registry get secret registry-auth -o jsonpath='{.data.password}' | base64 -d)"
```

and add `imagePullSecrets: [{name: registry}]` to the pod spec.

New password? Edit `password` in `registry/registry-auth`, then
`kubectl -n registry rollout restart deploy/registry`.

## Good to know

- DNS lists the workers only. Traefik still answers on every node, control
  planes included, so keep that in mind for the firewall.
- Traefik's load balancer listens on IPv4 only, so `dns.sh` removes AAAA
  records for the domain.
- The registry runs on one worker at a time, in the volume's location. If
  that worker goes away, it starts on another one there with the same
  images; with a single worker there, it waits for it to come back.
- Traefik closes a request after 60 seconds by default, so pushing a very
  large layer over a slow line can fail. Push from somewhere close to the
  cluster, or raise `entryPoints.websecure.transport.respondingTimeouts.readTimeout`
  with a k3s `HelmChartConfig` for Traefik.
- Let's Encrypt allows 5 certificates per exact set of names per week; avoid
  rebuilding more often than that.
