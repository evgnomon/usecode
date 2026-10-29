// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Signing up, logging in and subscribing, against a real Postgres. Runs when
// TEST_DATABASE_URL points at an empty database it may wipe; `make test-db`
// starts one. Stripe is answered by a fake fetch.

import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createApp, type Me } from "../server/app.js";
import { load } from "../server/config.js";
import { connect, migrate, type Sql } from "../server/db.js";
import type { Mailer } from "../server/mail.js";

const url = process.env.TEST_DATABASE_URL;
const root = fileURLToPath(new URL("../../", import.meta.url));
const PUBLIC = "http://console.test";
const cfg = load({
  PUBLIC_URL: PUBLIC,
  STRIPE_SECRET_KEY: "sk_test",
  STRIPE_PRICE_ID: "price_10",
  STRIPE_WEBHOOK_SECRET: "whsec_test",
});

let sql: Sql;
// Stands in for the pages Vite builds.
let client: string;
let app: ReturnType<typeof createApp>;
const stripeCalls: { method: string; path: string; body: string }[] = [];
let stripeSub = { id: "sub_1", customer: "cus_1", status: "active", cancel_at_period_end: false, items: { data: [{ current_period_end: 1_900_000_000 }] } };
const realFetch = globalThis.fetch;
/** What the console mailed, the newest last. */
const outbox: { to: string; subject: string; text: string }[] = [];
const mailer: Mailer = { send: async (to, subject, text) => void outbox.push({ to, subject, text }) };
/** The code in the last mail to `to`. */
const lastCode = (to: string) => outbox.findLast((m) => m.to === to)!.text.match(/\b\d{6}\b/)![0];

before(async () => {
  if (!url) return;
  sql = connect(url);
  await sql`DROP SCHEMA public CASCADE`;
  await sql`CREATE SCHEMA public`;
  assert.deepEqual(await migrate(sql, `${root}migrations`), ["0001_init.sql", "0002_email_codes.sql"]);
  assert.deepEqual(await migrate(sql, `${root}migrations`), [], "applied once");
  client = await mkdtemp(join(tmpdir(), "console-client-"));
  await writeFile(join(client, "index.html"), "<!doctype html><title>console</title>");
  await mkdir(join(client, "pages"));
  await writeFile(join(client, "pages/index.html"), "<!doctype html><title>home, prerendered</title>");
  await writeFile(join(client, "pages/404.html"), "<!doctype html><title>not found, prerendered</title>");
  await writeFile(join(client, "icon.svg"), "<svg/>");
  app = createApp(sql, cfg, client, mailer);
  globalThis.fetch = (async (input: string | URL, init?: RequestInit) => {
    const u = new URL(String(input));
    const path = u.pathname.replace(/^\/v1/, "");
    stripeCalls.push({ method: init?.method ?? "GET", path, body: String(init?.body ?? "") });
    const json = (v: unknown) => new Response(JSON.stringify(v), { headers: { "content-type": "application/json" } });
    if (path === "/customers") return json({ id: "cus_1" });
    if (path === "/checkout/sessions") return json({ url: "https://checkout.stripe.com/c/pay/cs_1" });
    if (path === "/billing_portal/sessions") return json({ url: "https://billing.stripe.com/p/session/1" });
    if (path === "/subscriptions/sub_1") return json(stripeSub);
    return new Response(JSON.stringify({ error: { message: "unknown" } }), { status: 404 });
  }) as typeof fetch;
});

after(async () => {
  globalThis.fetch = realFetch;
  await sql?.end();
  if (client) await rm(client, { recursive: true });
});

/** A browser: posts JSON from the console's own origin and keeps the cookie. */
function browser() {
  const jar = new Map<string, string>();
  return async (path: string, body?: Record<string, string>) => {
    const cookie = [...jar].map(([k, v]) => `${k}=${v}`).join("; ");
    const res = await app.request(`${PUBLIC}${path}`, {
      method: body ? "POST" : "GET",
      headers: {
        ...(cookie ? { cookie } : {}),
        ...(body ? { origin: PUBLIC, "content-type": "application/json" } : {}),
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    for (const set of res.headers.getSetCookie()) {
      const [pair] = set.split(";");
      const [k, v] = [pair!.slice(0, pair!.indexOf("=")), pair!.slice(pair!.indexOf("=") + 1)];
      if (v) jar.set(k, v);
      else jar.delete(k);
    }
    return res;
  };
}

/** Signs up or logs in all the way, with the code from the mail. */
async function through(b: ReturnType<typeof browser>, path: "/api/signup" | "/api/login", email: string, password: string) {
  const first = await b(path, { email, password });
  assert.equal(first.status, 202, `${path}: ${await first.clone().text()}`);
  return b("/api/verify", { code: lastCode(email.toLowerCase()) });
}

function webhook(event: unknown, secret = "whsec_test") {
  const body = JSON.stringify(event);
  const t = Math.floor(Date.now() / 1000);
  const v1 = createHmac("sha256", secret).update(`${t}.${body}`).digest("hex");
  return app.request(`${PUBLIC}/stripe/webhook`, {
    method: "POST",
    headers: { "content-type": "application/json", "stripe-signature": `t=${t},v1=${v1}` },
    body,
  });
}

test("the pages and their files are served", { skip: !url }, async () => {
  const get = browser();
  for (const path of ["/pricing", "/login", "/signup"]) {
    const res = await get(path);
    assert.equal(res.status, 200, path);
    assert.match(await res.text(), /<title>console<\/title>/, path);
    assert.equal(res.headers.get("cache-control"), "no-cache", path);
  }
  const home = await get("/");
  assert.equal(home.status, 200);
  assert.match(await home.text(), /<title>home, prerendered<\/title>/, "a prerendered page wins");
  const missing = await get("/nope");
  assert.equal(missing.status, 404);
  assert.match(await missing.text(), /<title>not found, prerendered<\/title>/);
  assert.equal((await get("/api/nope")).status, 404);
  assert.equal((await get("/healthz")).status, 200);
  assert.match((await get("/icon.svg")).headers.get("content-type") ?? "", /^image\/svg\+xml/);
  assert.equal((await get("/nope.js")).status, 404);
  const account = await get("/account");
  assert.equal(account.status, 302);
  assert.equal(account.headers.get("location"), "/login?next=/account");
  const me = (await (await get("/api/me")).json()) as Me;
  assert.deepEqual(me, { user: null, billing: true, subscription: null, subscribed: false });
});

test("sign up, log out, log in", { skip: !url }, async () => {
  const b = browser();
  const short = await b("/api/signup", { email: "Ada@Example.com", password: "short" });
  assert.equal(short.status, 422);
  assert.match(((await short.json()) as { error: string }).error, /10 characters/);
  const asked = await b("/api/signup", { email: "Ada@Example.com", password: "a long password" });
  assert.equal(asked.status, 202);
  assert.equal(outbox.at(-1)!.to, "ada@example.com");
  assert.match(outbox.at(-1)!.subject, /^\d{6} is your usecode\.dev code$/);
  assert.equal((await b("/api/me").then((r) => r.json()) as Me).user, null, "no account before the code");
  assert.equal((await browser()("/api/login", { email: "ada@example.com", password: "a long password" })).status, 401, "nor a login");
  const good = lastCode("ada@example.com");
  const bad = good === "000000" ? "111111" : "000000";
  assert.equal((await b("/api/verify", { code: bad })).status, 401);
  assert.equal((await browser()("/api/verify", { code: good })).status, 410, "the code needs its browser");
  const made = await b("/api/verify", { code: `${good.slice(0, 3)} ${good.slice(3)}` });
  assert.equal(made.status, 201);
  assert.equal((await b("/api/verify", { code: good })).status, 410, "a code works once");
  assert.equal(((await (await b("/api/me")).json()) as Me).user?.email, "ada@example.com");
  assert.equal((await b("/login")).headers.get("location"), "/account", "logged in already");
  assert.equal((await b("/account")).status, 200);

  assert.equal((await browser()("/api/signup", { email: "ada@example.com", password: "another password" })).status, 409);
  const sent = outbox.length;

  assert.equal((await b("/api/logout", {})).status, 204);
  assert.equal((await b("/account")).status, 302, "the session is gone");

  assert.equal((await b("/api/login", { email: "ada@example.com", password: "wrong password!" })).status, 401);
  assert.equal(outbox.length, sent, "no code for a wrong password");
  assert.equal((await b("/api/login", { email: "ADA@example.com", password: "a long password" })).status, 202);
  assert.equal((await b("/account")).status, 302, "the password alone doesn't log in");
  assert.equal((await b("/api/verify", { code: lastCode("ada@example.com") })).status, 200);
  assert.equal((await b("/account")).status, 200);
});

test("a post from another site, or not in JSON, is refused", { skip: !url }, async () => {
  const signup = (headers: Record<string, string>, body: string) =>
    app.request(`${PUBLIC}/api/signup`, { method: "POST", headers, body });
  const creds = { email: "x@example.com", password: "a long password" };
  const form = await signup(
    { origin: "https://evil.example", "content-type": "application/x-www-form-urlencoded" },
    new URLSearchParams(creds).toString(),
  );
  assert.equal(form.status, 403);
  assert.equal((await signup({ origin: PUBLIC, "content-type": "text/plain" }, JSON.stringify(creds))).status, 415);
  assert.equal((await signup({ origin: PUBLIC }, JSON.stringify(creds))).status, 415);
});

test("subscribe through Stripe and see it on the account", { skip: !url }, async () => {
  const b = browser();
  const me = async () => (await (await b("/api/me")).json()) as Me;
  const go = async (path: string) => ((await (await b(path, {})).json()) as { url: string }).url;
  assert.equal((await through(b, "/api/signup", "grace@example.com", "a long password")).status, 201);
  assert.equal(await go("/api/billing/checkout"), "https://checkout.stripe.com/c/pay/cs_1");
  const session = stripeCalls.find((c) => c.path === "/checkout/sessions")!;
  assert.match(session.body, /customer=cus_1/);
  assert.match(session.body, /line_items%5B0%5D%5Bprice%5D=price_10/);
  assert.equal((await me()).subscribed, false);

  assert.equal((await webhook({ id: "evt_1", type: "checkout.session.completed", data: { object: {} } }, "whsec_nope")).status, 400);
  const completed = { id: "evt_1", type: "checkout.session.completed", data: { object: { subscription: "sub_1" } } };
  assert.equal((await webhook(completed)).status, 200);
  assert.deepEqual(await me(), {
    user: { email: "grace@example.com" },
    billing: true,
    subscription: { status: "active", currentPeriodEnd: "2030-03-17T17:46:40.000Z", cancelAtPeriodEnd: false },
    subscribed: true,
  });
  assert.equal(await go("/api/billing/checkout"), "/account", "no second subscription");
  assert.equal(await go("/api/billing/portal"), "https://billing.stripe.com/p/session/1");

  // A late duplicate of the first event changes nothing; the cancel does.
  stripeSub = { ...stripeSub, status: "canceled" };
  const fetched = stripeCalls.length;
  assert.equal((await webhook({ id: "evt_2", type: "customer.subscription.deleted", data: { object: { id: "sub_1" } } })).status, 200);
  assert.equal((await webhook(completed)).status, 200);
  assert.equal(stripeCalls.length, fetched + 2);
  assert.equal((await me()).subscribed, false);
});

test("a code takes a few wrong guesses, then it's gone", { skip: !url }, async () => {
  const b = browser();
  assert.equal((await through(b, "/api/signup", "linus@example.com", "a long password")).status, 201);
  await b("/api/logout", {});
  assert.equal((await b("/api/login", { email: "linus@example.com", password: "a long password" })).status, 202);
  const good = lastCode("linus@example.com");
  const bad = good === "000000" ? "111111" : "000000";
  for (let i = 1; i < 5; i++) assert.equal((await b("/api/verify", { code: bad })).status, 401, `guess ${i}`);
  assert.equal((await b("/api/verify", { code: bad })).status, 410, "the fifth ends it");
  assert.equal((await b("/api/verify", { code: good })).status, 410, "even the right one is too late");
  assert.equal((await through(b, "/api/login", "linus@example.com", "a long password")).status, 200, "a new code works");
});

test("a failed mail says so and logs nobody in", { skip: !url }, async () => {
  const broken = createApp(sql, cfg, client, { send: async () => { throw new Error("smtp down"); } });
  const res = await broken.request(`${PUBLIC}/api/signup`, {
    method: "POST",
    headers: { origin: PUBLIC, "content-type": "application/json" },
    body: JSON.stringify({ email: "nomail@example.com", password: "a long password" }),
  });
  assert.equal(res.status, 503);
  assert.equal(res.headers.getSetCookie().length, 0);
});

test("billing needs a login", { skip: !url }, async () => {
  assert.equal((await browser()("/api/billing/checkout", {})).status, 401);
});
