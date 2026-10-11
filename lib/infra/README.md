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

## Settings for `uc kube configure`

Everything below is switched on from your user config
(`~/src/github.com/<you>/config/config.yaml`, also at `~/.config/usecode`),
so a plain `uc kube configure` keeps the whole cluster in shape. Settings
for one cluster go under `kube.<context>`; settings at the top level count
for every cluster:

```yaml
kube:
  cp-1:
    registry_host: registry.example.com
    registry_dns_target: example.com
    obs_enabled: true
    obs_alert_webhook: https://agent.example.com/alerts
```

Want to try something once first? `-e key=value` wins over the config for
that run. A long config can be split up: `include: [kube.yaml]` (or a
directory of `*.yaml`) pulls other files in, and the including file wins.

## Your registry

A private registry for your own images, with its images on a 10Gi Hetzner
Cloud Volume, so it can move to another worker and take them along:

```yaml
kube:
  cp-1:
    registry_host: registry.example.com
    registry_dns_target: example.com
```

```bash
source ~/.bashrc.d/cloudflare.sh
uc kube configure -t registry
```

`registry_dns_target` makes the name a CNAME to your domain, so it follows
the workers by itself. Want more room? `registry_size: 50Gi` grows the
volume (it never shrinks).

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

## Your mail server

Your own mail, with [Stalwart](https://stalw.art) (SMTP, IMAP and JMAP in
one), on a 10Gi Hetzner Cloud Volume, so it can move to another worker
and take your mail along:

```yaml
kube:
  cp-1:
    mail_host: mail.example.com
    mail_node: worker-1        # optional: keep Stalwart on this worker
```

```bash
uc kube configure -t mail
```

The web admin is at `https://mail.example.com/admin`. The first run makes a
recovery admin, user `admin` with a random password:

```bash
kubectl -n mail get secret mail-admin -o jsonpath='{.data.password}' | base64 -d
```

Log in and go through Stalwart's wizard: use `mail.example.com` as the
hostname and keep the HTTP listener on 8080 (Traefik talks to it). Then
add your domain, and let `uc mail configure` do the part that decides
whether your mail is trusted:

```bash
source ~/.bashrc.d/cloudflare.sh
uc mail configure -C     # see what it would change
uc mail configure
```

It points `mail.example.com` at the worker Stalwart answers on, and gives
each domain its MX, SPF, DKIM, DMARC (`p=reject`; `mail_dmarc_policy`
changes it), TLS reporting and the SRV records mail apps look for. It
also sets the workers' reverse DNS to the mail name on Hetzner, hands the
mail ports the Let's Encrypt certificate the cluster gets for the name, and
switches off Stalwart's own DNS, DKIM rotation and ACME, which would fight
it. A domain whose NS records aren't Cloudflare's gets the records to add
by hand instead. Run it again after adding a domain, once the certificate
is first issued, and if Stalwart moves to another worker.

The worker's address is the mail name's, so Stalwart moving would leave DNS
behind. `mail_node` pins it to one worker so that never happens; without
it, the role's check fails as soon as the mail name and the mail ports
disagree, and `uc mail configure` puts them back together.

Ports 25, 465, 587, 993 and 995 answer on the worker Stalwart runs on, so
it sees who's really sending. One thing only Hetzner can do: they block
outgoing mail on new projects until you ask.

Once you have an admin of your own, `mail_recovery_admin: false` drops the
recovery login (then give `uc mail configure` yours in `UC_MAIL_USER` and
`UC_MAIL_PASSWORD`). When cert-manager renews the certificate, run the role
again so Stalwart picks it up.

## Your name servers

Your own authoritative DNS, with [PowerDNS](https://www.powerdns.com),
when you'd like your zones to live on your cluster too. You get two name
servers that share nothing: each has its own SQLite database on its own
10Gi Hetzner Cloud Volume, and they never run on the same node, so losing
one leaves the other answering. They're on for every cluster, so a plain
`uc kube configure` (or just this part of it) sets them up:

```bash
uc kube configure -t dns
```

Rather not have them? `dns_enabled: false` at the top level of your
config turns them off everywhere, or under `kube.<context>` for one
cluster.

Like Traefik on 80 and 443, port 53 (UDP and TCP) answers on every node
and hands each query to whichever instance is up, wherever it runs. So
your name servers are simply the workers `$INFRA_DOMAIN` already points
at. The volumes are spread over your locations (`dns_locations: [nbg1,
fsn1]` picks them), and `dns_instances: 3` adds one. There's nothing to
open in the hosts' firewalls: like 80 and 443, port 53 is forwarded to the
pods before the daemon's inbound chain sees it.

Then look after your zones with `uc dns`, which makes every change on
every name server:

```bash
uc dns servers                         # where they run and answer
uc dns zone add example.com            # ns1, ns2 on the workers, with glue
uc dns add example.com @ A 192.0.2.10
uc dns add example.com www CNAME @
uc dns add example.com @ MX '10 mail'
uc dns set example.com @ TXT 'v=spf1 mx -all'
uc dns rm example.com www CNAME
uc dns list example.com
```

`zone add` prints the name servers and glue to give your registrar. If a
name server was down while you changed something, `uc dns list` points out
where they disagree, and `uc dns sync --from pdns-1` catches the other one
up (`-C` shows what it would do first).

## Metrics, logs and alerts

A small monitoring stack with no Prometheus server and no UI to look after.
Everything goes through kubectl, which makes it easy for you and for your
agents to ask questions:

```yaml
kube:
  cp-1:
    obs_enabled: true
    obs_alert_webhook: https://agent.example.com/alerts
```

```bash
uc kube configure -t obs
```

You get node-exporter and kube-state-metrics, VictoriaMetrics to scrape
and keep them (30 days), vmalert with a handful of alerts (nodes, disks,
memory, volumes, crash loops), and VictoriaLogs with a collector on every
node for all container logs (14 days). Metrics and logs each sit on a 10Gi
Hetzner Cloud Volume, in two different locations: metrics in your primary
location, logs in the next one with workers. Losing a whole location costs
you one of them, never both. Want them elsewhere? `obs_metrics_location`
and `obs_logs_location` (e.g. `fsn1`, `hel1`). Only have workers in one
location? Set `obs_logs_location` to it and both live there.

Ask it something:

```bash
kubectl get --raw '/api/v1/namespaces/obs/services/vmsingle:8428/proxy/api/v1/query?query=up'
kubectl get --raw '/api/v1/namespaces/obs/services/vlsingle:9428/proxy/select/logsql/query?query=_time:5m%20error&limit=20'
```

Your own apps get scraped once their Service (or Pod) has
`prometheus.io/scrape: "true"` and `prometheus.io/port: "<port>"`.
Firing alerts are POSTed to `<obs_alert_webhook>/api/v2/alerts`, the way
Alertmanager would get them; without a webhook they're still evaluated and
kept as the `ALERTS` series. More room or time: `obs_metrics_size`,
`obs_logs_size`, `obs_metrics_retention`, `obs_logs_retention`. Your own
alerts: `obs_alert_groups`, in the Prometheus rules format.

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
