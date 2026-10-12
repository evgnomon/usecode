// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { test } from "node:test";
import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { hashPassword, normalizeEmail, signupProblem, verifyPassword } from "../server/auth.js";
import { isLive, verifySignature } from "../server/billing.js";
import { Limiter } from "../server/app.js";
import { load } from "../server/config.js";
import { safeNext } from "../src/lib/safe-next.js";

test("a password checks against its own hash only", async () => {
  const hash = await hashPassword("correct horse battery");
  assert.match(hash, /^scrypt\$32768\$8\$1\$/);
  assert.equal(await verifyPassword("correct horse battery", hash), true);
  assert.equal(await verifyPassword("correct horse batterY", hash), false);
  assert.equal(await verifyPassword("anything", "garbage"), false);
  assert.notEqual(await hashPassword("correct horse battery"), hash, "salted");
});

test("signup wants an email and a long enough password", () => {
  assert.equal(signupProblem("a@b.co", "0123456789"), null);
  assert.match(signupProblem("not-an-email", "0123456789")!, /email/);
  assert.match(signupProblem("a@b.co", "short")!, /10 characters/);
  assert.equal(normalizeEmail("  Me@Example.COM "), "me@example.com");
});

test("a login only sends you somewhere on this site", () => {
  assert.equal(safeNext("/pricing"), "/pricing");
  assert.equal(safeNext("//evil.example"), "/account");
  assert.equal(safeNext("/\\evil.example"), "/account");
  assert.equal(safeNext("https://evil.example"), "/account");
  assert.equal(safeNext(undefined), "/account");
});

test("too many failed logins block the email for a while", () => {
  const l = new Limiter(3, 1000);
  for (let i = 0; i < 3; i++) l.fail("a", 0);
  assert.equal(l.blocked("a", 10), true);
  assert.equal(l.blocked("b", 10), false);
  assert.equal(l.blocked("a", 2000), false);
});

test("a Stripe signature is checked against the body, secret and time", () => {
  const body = '{"id":"evt_1"}';
  const sign = (t: number, secret = "whsec_x") =>
    `t=${t},v1=${createHmac("sha256", secret).update(`${t}.${body}`).digest("hex")}`;
  assert.equal(verifySignature(body, sign(1000), "whsec_x", 1000), true);
  assert.equal(verifySignature(body, `${sign(1000)},v0=abc`, "whsec_x", 1000), true);
  assert.equal(verifySignature(body + " ", sign(1000), "whsec_x", 1000), false);
  assert.equal(verifySignature(body, sign(1000, "whsec_y"), "whsec_x", 1000), false);
  assert.equal(verifySignature(body, sign(1000), "whsec_x", 2000), false, "too old");
  assert.equal(verifySignature(body, "", "whsec_x", 1000), false);
});

test("past due still counts as subscribed, cancelled doesn't", () => {
  const sub = (status: string) => ({ status, currentPeriodEnd: null, cancelAtPeriodEnd: false });
  assert.equal(isLive(sub("active")), true);
  assert.equal(isLive(sub("past_due")), true);
  assert.equal(isLive(sub("canceled")), false);
  assert.equal(isLive(null), false);
});

test("billing is on only with all three Stripe settings", () => {
  assert.equal(load({ STRIPE_SECRET_KEY: "sk", STRIPE_PRICE_ID: "price" }).stripe, null);
  const cfg = load({
    PUBLIC_URL: "https://usecode.dev/",
    STRIPE_SECRET_KEY: "sk",
    STRIPE_PRICE_ID: "price",
    STRIPE_WEBHOOK_SECRET: "whsec",
  });
  assert.equal(cfg.publicUrl, "https://usecode.dev");
  assert.equal(cfg.secureCookies, true);
  assert.deepEqual(cfg.stripe, { secretKey: "sk", priceId: "price", webhookSecret: "whsec" });
});
