<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# console

The usecode.dev you land on: the landing page, the pricing, and signing up
and logging in. An individual account is free, Hosted is $10 a month per
user, and Enterprise is a conversation. Either way usecode.dev looks after
all your nodes, edge or Kubernetes, however many you add.

The pages are a [Vite](https://vite.dev) + React app styled with
[Tailwind CSS](https://tailwindcss.com) and [shadcn/ui](https://ui.shadcn.com)
(in `src/`), served by a small [Hono](https://hono.dev) server (in
`server/`). The server keeps no data: everything with state, accounts,
sessions, billing and nodes, is the usecode API's (`lib/api`, in Rust), and
the pages call it straight from the browser, at `https://api.usecode.dev`.
The server is there for the pages: it sends them prerendered where it can,
tells them where the API is, and asks the API who's logged in so `/account`
and `/login` send you to the right one before anything loads.

Payments go through Stripe Checkout, and the customer portal handles card
changes and cancelling, both through the API.

## Try it

```sh
make -C ../api test-db   # a throwaway Postgres for the API
USECODE_AGENT_DATABASE_URL=postgres://postgres:test@127.0.0.1:55433/api \
  USECODE_AGENT_NODE_NAME=api-1 make -C ../api run   # the API on :8000
make serve               # http://localhost:8080
make dev                 # the pages with hot reload on :5173
make test
```

## Settings

| Variable           | What it is                                                     |
| ------------------ | -------------------------------------------------------------- |
| `PUBLIC_URL`       | Where it's reached, e.g. `https://usecode.dev`                 |
| `API_URL`          | Where browsers reach the API, default `http://localhost:8000`  |
| `API_INTERNAL_URL` | Where this server reaches it, e.g. `http://api:8000`; default `API_URL` |
| `PORT`             | Default `8080`                                                 |

Mail, Stripe and the database are the API's settings; see `lib/api/README.md`.

Signing up and logging in both take the password and then a six digit code
mailed to the address, good for 10 minutes and five guesses. The account is
only made once the code is right. A forgotten password is reset at
`/forgot`: the address and a new password, then the code. The new password
only takes once the code is right, and it logs the account out everywhere
else. An address without an account gets the same answers and no mail, so
the page doesn't tell who has one.

In Stripe, point a webhook endpoint at `https://api.usecode.dev/stripe/webhook`
with `checkout.session.completed` and `customer.subscription.created`,
`.updated` and `.deleted`, and switch the customer portal on. The old
address, `https://usecode.dev/stripe/webhook`, is handed on to the API
until you've moved it.

## Ship it

Commit and `git push` your change, then, with your kube context on the
cluster, `make deploy` at the top of the checkout: it ships the API first,
then the console. `make deploy` here ships only the console.

That's `scripts/deploy.sh`: it builds the image at the latest push
(origin/master, not the working tree, and it warns about anything local it
leaves out), pushes it as `<sha>` and `latest`, points `console/console` at
`<sha>`, waits for the rollout and checks `https://<host>/healthz` answers.
The steps also run on their own: `make image`, `make push`, `make rollout`.

`make push` reads the registry host from `CONTAINER_REGISTRY` (falling back
to the `registry` Ingress's host if it's unset), logs podman in with the
login the cluster keeps in the `registry-auth` secret (namespace `registry`)
and pushes `<sha>` and `latest`.

`make rollout` changes the image tag to that sha and waits until the new
replicas are Running. Verify with `curl -fsSL https://<host>/ | head` (the landing page
is prerendered, so new copy and fonts show straight away) or
`kubectl -n console get pods`.

Alternatively `uc kube sync -t console` reconciles the whole role — image,
Postgres, Ingress, certificate — and pins a build with
`-e console_image=$CONTAINER_REGISTRY/usecode-console:<sha>`.

The `console` role runs it next to its own Postgres on a Hetzner Volume,
behind Traefik with a Let's Encrypt certificate, on the name your config
gives as `console_host` under `kube.<context>`, e.g. `console_host: usecode.dev`
(`console_image` goes there too). Without it the role leaves the cluster be;
see `lib/uc/src/kube/roles/console.rs` for the rest.

Copy lives in `src/content.ts` and the pages are in `src/pages/`. The API
they call is `lib/api`, through `src/lib/api.tsx`.

The landing page, pricing and the 404 are rendered to plain HTML at build
time (`src/prerender.tsx`, the pages marked `prerender` in `src/App.tsx`),
so search engines, AI tools and `curl` read the whole page without running
any JavaScript. The browser then picks them up and carries on as an app.
The account and login pages stay browser-only.

The look is one set of tokens in `src/index.css`, light and dark, mapped
onto shadcn/ui's variables, so a component from `npx shadcn add <name>`
picks the theme up as it is. Corners are square everywhere (`--radius: 0`).
The console's pieces
(`NodeRow`, `OptionRow`, `Activity`, ...) are in `src/components/console.tsx`
and shadcn's own are in `src/components/ui/`.
