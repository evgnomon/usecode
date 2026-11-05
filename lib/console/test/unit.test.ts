// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { test } from "node:test";
import assert from "node:assert/strict";
import { withApi } from "../server/app.js";
import { load } from "../server/config.js";
import { safeNext } from "../src/lib/safe-next.js";

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
