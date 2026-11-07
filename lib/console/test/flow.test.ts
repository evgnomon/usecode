// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The pages as the server sends them, with the API answered by a fake.
// Signing up, logging in and billing are the API's and tested there
// (lib/api, `make -C lib/api test`).

import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createApp, type Fetch } from "../server/app.js";
import { load } from "../server/config.js";

const cfg = load({ PUBLIC_URL: "http://console.test", API_URL: "http://api.test", API_INTERNAL_URL: "http://api.internal" });

let client: string;
let app: ReturnType<typeof createApp>;
/** What the fake API was asked. */
const calls: { url: string; init?: RequestInit }[] = [];
/** Whether the fake API is up. */
let apiUp = true;

const json = (v: unknown, status = 200) => new Response(JSON.stringify(v), { status, headers: { "content-type": "application/json" } });

const fakeApi: Fetch = async (url, init) => {
  calls.push({ url, init });
  if (!apiUp) throw new Error("connection refused");
  const cookie = new Headers(init?.headers).get("cookie") ?? "";
  if (url === "http://api.internal/account") {
    return json({ user: cookie.includes("uc_session=good") ? { email: "ada@example.com" } : null, billing: false, subscription: null, subscribed: false });
  }
  if (url === "http://api.internal/stripe/webhook") return json({ received: true });
  return json({ detail: "Not Found" }, 404);
};

before(async () => {
  client = await mkdtemp(join(tmpdir(), "console-client-"));
  await writeFile(join(client, "index.html"), "<!doctype html><html><head><title>console</title></head></html>");
  await mkdir(join(client, "pages"));
  await writeFile(join(client, "pages/index.html"), "<!doctype html><html><head><title>home, prerendered</title></head></html>");
  await writeFile(join(client, "pages/404.html"), "<!doctype html><html><head><title>not found, prerendered</title></head></html>");
  await writeFile(join(client, "icon.svg"), "<svg/>");
  await writeFile(join(client, "manifest.webmanifest"), "{}");
  await writeFile(join(client, "sw.js"), "");
  app = createApp(cfg, client, fakeApi);
});

after(async () => {
  if (client) await rm(client, { recursive: true });
});

const get = (path: string, cookie?: string) => app.request(path, { headers: cookie ? { cookie } : {} });

test("pages are prerendered when they can be, and know where the API is", async () => {
  const home = await get("/");
  assert.equal(home.status, 200);
  const html = await home.text();
  assert.match(html, /home, prerendered/);
  assert.match(html, /<meta name="uc-api" content="http:\/\/api.test" \/>/);
  assert.match(home.headers.get("content-security-policy") ?? "", /connect-src 'self' http:\/\/api.test/);

  const pricing = await get("/pricing");
  assert.equal(pricing.status, 200);
  assert.match(await pricing.text(), /<title>console<\/title>/, "the app renders it in the browser");

  const missing = await get("/nope");
  assert.equal(missing.status, 404);
  assert.match(await missing.text(), /not found, prerendered/);

  assert.equal((await get("/icon.svg")).status, 200);
  const manifest = await get("/manifest.webmanifest");
  assert.equal(manifest.status, 200, "the app is installable");
  assert.match(manifest.headers.get("content-type") ?? "", /application\/manifest\+json/);
  assert.match((await get("/sw.js")).headers.get("content-type") ?? "", /javascript/);
  assert.equal(await (await get("/healthz")).text(), "ok");
});

test("the account is for the logged in, the login for the others", async () => {
  calls.length = 0;
  const visitor = await get("/account");
  assert.equal(visitor.status, 302);
  assert.equal(visitor.headers.get("location"), "/login?next=/account");
  assert.equal(calls.length, 0, "no session cookie, no need to ask");

  const expired = await get("/account", "uc_session=old");
  assert.equal(expired.headers.get("location"), "/login?next=/account");
  assert.equal(calls.length, 1, "asked the API inside the cluster");
  assert.equal(new Headers(calls[0]!.init?.headers).get("cookie"), "uc_session=old");

  assert.equal((await get("/account", "uc_session=good")).status, 200);
  assert.equal((await get("/hosts")).headers.get("location"), "/login?next=/hosts");
  assert.equal((await get("/hosts", "uc_session=good")).status, 200);
  assert.equal((await get("/workspaces")).headers.get("location"), "/login?next=/workspaces");
  assert.equal(
    (await get("/shell?host=h&ws=dev")).headers.get("location"),
    "/login?next=/shell%3Fhost%3Dh%26ws%3Ddev",
    "back to the same shell after logging in",
  );
  const member = await get("/login", "uc_session=good");
  assert.equal(member.status, 302);
  assert.equal(member.headers.get("location"), "/account");
  assert.equal((await get("/signup")).status, 200);
});

test("only the shell's page may run WebAssembly and inline styles", async () => {
  const csp = async (path: string) => (await get(path, "uc_session=good")).headers.get("content-security-policy") ?? "";
  const shell = await csp("/shell?host=h&ws=dev");
  assert.match(shell, /script-src 'self' 'wasm-unsafe-eval'/);
  assert.match(shell, /style-src 'self' 'unsafe-inline'/);
  assert.match(shell, /connect-src 'self' http:\/\/api.test ws:\/\/api.test/);
  for (const path of ["/", "/workspaces", "/hosts"]) {
    const strict = await csp(path);
    assert.doesNotMatch(strict, /wasm-unsafe-eval|unsafe-inline/, path);
  }
});

test("when the API is down, nobody is sent anywhere", async () => {
  apiUp = false;
  try {
    assert.equal((await get("/account", "uc_session=good")).status, 200);
    assert.equal((await get("/login", "uc_session=good")).status, 200);
  } finally {
    apiUp = true;
  }
});

test("Stripe's webhook is handed to the API as it came", async () => {
  calls.length = 0;
  const res = await app.request("/stripe/webhook", {
    method: "POST",
    headers: { "content-type": "application/json", "stripe-signature": "t=1,v1=ab" },
    body: '{"id":"evt_1"}',
  });
  assert.equal(res.status, 200);
  assert.deepEqual(await res.json(), { received: true });
  const call = calls[0]!;
  assert.equal(call.url, "http://api.internal/stripe/webhook");
  assert.equal(new Headers(call.init?.headers).get("stripe-signature"), "t=1,v1=ab");
  assert.equal(call.init?.body, '{"id":"evt_1"}');
});
