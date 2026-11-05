// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { test } from "node:test";
import assert from "node:assert/strict";
import { withApi } from "../server/app.js";
import { load } from "../server/config.js";
import { safeNext } from "../src/lib/safe-next.js";
import { ago, duration, hostHealth, hostsPath, pageRange, type Host } from "../src/lib/hosts.js";

test("a login only sends you somewhere on this site", () => {
  assert.equal(safeNext("/pricing"), "/pricing");
  assert.equal(safeNext("//evil.example"), "/account");
  assert.equal(safeNext("/\\evil.example"), "/account");
  assert.equal(safeNext("https://evil.example"), "/account");
  assert.equal(safeNext(undefined), "/account");
});

test("the API is where the settings say, inside the cluster too", () => {
  const local = load({});
  assert.equal(local.apiUrl, "http://localhost:8000");
  assert.equal(local.apiInternalUrl, "http://localhost:8000");
  const live = load({ PUBLIC_URL: "https://usecode.dev/", API_URL: "https://api.usecode.dev/", API_INTERNAL_URL: "http://api:8000" });
  assert.equal(live.publicUrl, "https://usecode.dev");
  assert.equal(live.apiUrl, "https://api.usecode.dev");
  assert.equal(live.apiInternalUrl, "http://api:8000");
});

test("a page carries the API's address", () => {
  const html = withApi('<html><head><title>x</title></head></html>', 'https://api.usecode.dev/"x');
  assert.match(html, /<head>\s*<meta name="uc-api" content="https:\/\/api.usecode.dev\/&quot;x" \/>/);
});

test("a page of hosts is asked for with only what differs from the defaults", () => {
  assert.equal(hostsPath({ search: "", filter: "all", sort: "name", offset: 0, limit: 50 }), "/hosts?limit=50");
  assert.equal(
    hostsPath({ search: " web 1 ", filter: "offline", sort: "quiet", offset: 100, limit: 50 }),
    "/hosts?q=web+1&state=offline&sort=quiet&offset=100&limit=50",
  );
});

test("hosts read as online, failing or offline, with times people say", () => {
  const host = { online: true, failing: false } as Host;
  assert.equal(hostHealth(host), "online");
  assert.equal(hostHealth({ ...host, failing: true }), "failing");
  assert.equal(hostHealth({ ...host, online: false, failing: false }), "offline");

  assert.equal(ago(1000, 1010), "just now");
  assert.equal(ago(1000, 1000 + 4 * 60), "4 min ago");
  assert.equal(ago(1000, 1000 + 3 * 3600 + 5), "3 h ago");
  assert.equal(ago(1000, 1000 + 2 * 86400), "2 d ago");
  assert.equal(duration(300), "5 min");
  assert.equal(duration(3 * 3600 + 20 * 60), "3 h 20 min");
  assert.equal(duration(12 * 86400 + 4 * 3600), "12 d 4 h");
  assert.equal(pageRange(0, 50, 1000), "1–50 of 1,000");
  assert.equal(pageRange(950, 50, 1000), "951–1,000 of 1,000");
  assert.equal(pageRange(0, 0, 0), "none");
});
