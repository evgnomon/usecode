<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# console

The usecode.dev you land on: the landing page, the pricing, and signing up
and logging in. An individual account is free, Team is $10 a month per
user, and Enterprise is a conversation. Either way usecode.dev looks after
all your nodes, edge or Kubernetes, however many you add.

All TypeScript. The pages are a [Vite](https://vite.dev) + React app styled
with [Tailwind CSS](https://tailwindcss.com) and
[shadcn/ui](https://ui.shadcn.com) (in `src/`). Behind them is a small
[Hono](https://hono.dev) server (in `server/`) with a JSON API, Postgres
through [postgres](https://github.com/porsager/postgres), and the built
pages. Payments go through Stripe Checkout, and the customer portal handles
card changes and cancelling.

## Try it

```sh
make test-db      # a throwaway Postgres in podman, on 127.0.0.1:55432
make serve        # http://localhost:8080
make dev          # the pages with hot reload on :5173, next to `make serve`
make test         # unit tests, plus the sign up and billing flow against test-db
make test-db-stop
```

## Settings

| Variable                | What it is                                                   |
| ----------------------- | ------------------------------------------------------------ |
| `PUBLIC_URL`            | Where it's reached, e.g. `https://usecode.dev`               |
| `DATABASE_URL`          | A `postgres://` URL; else `PGHOST`, `PGUSER`, ... are used   |
| `PORT`                  | Default `8080`                                               |
| `STRIPE_SECRET_KEY`     | Billing is on once all three Stripe settings are set         |
| `STRIPE_PRICE_ID`       | The Team plan's $10/month per user recurring price          |
| `STRIPE_WEBHOOK_SECRET` | The signing secret of the webhook endpoint                   |

Migrations in `migrations/` run on start, once, even with several replicas.

In Stripe, point a webhook endpoint at `https://usecode.dev/stripe/webhook`
with `checkout.session.completed` and `customer.subscription.created`,
`.updated` and `.deleted`, and switch the customer portal on.

## Ship it

```sh
make push                          # <registry_host>/usecode-console:<sha> and :latest
STRIPE_SECRET_KEY=... STRIPE_PRICE_ID=... STRIPE_WEBHOOK_SECRET=... \
  uc kube configure -t console
```

`make push` sends the image to your cluster's own registry (the `registry`
role), logged in with the password the cluster keeps, and the role pulls it
from there with the same login. Pin a build with
`-e console_image=<registry_host>/usecode-console:<sha>`.

The `console` role runs it next to its own Postgres on a Hetzner Volume,
behind Traefik with a Let's Encrypt certificate, on the name your config
gives as `console_host` under `kube.<context>`, e.g. `console_host: usecode.dev`
(`console_image` goes there too). Without it the role leaves the cluster be;
see `lib/uc/src/kube/roles/console.rs` for the rest.

Copy lives in `src/content.ts` and the pages are in `src/pages/`. The API
they call is in `server/app.ts`.

The landing page, pricing and the 404 are rendered to plain HTML at build
time (`src/prerender.tsx`, the pages marked `prerender` in `src/App.tsx`),
so search engines, AI tools and `curl` read the whole page without running
any JavaScript. The browser then picks them up and carries on as an app.
The account and login pages stay browser-only.

The look is one set of tokens in `src/index.css`, light and dark, mapped
onto shadcn/ui's variables, so a component from `npx shadcn add <name>`
picks the theme up as it is. `/design` shows all of it: the tokens, the
status pills, and example screens of a cluster with a node joining, an
option being applied and a node that drifted. The console's pieces
(`NodeRow`, `OptionRow`, `Activity`, ...) are in `src/components/console.tsx`
and shadcn's own are in `src/components/ui/`.
