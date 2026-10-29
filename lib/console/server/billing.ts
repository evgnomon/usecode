// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The Hosted plan: $10 a month per user, any number of nodes. Individual
// accounts are free and never come here.
// Stripe holds the truth; Checkout takes the payment, the customer portal
// handles card changes and cancelling, and the webhook copies the
// subscription's state here.

import { createHmac, timingSafeEqual } from "node:crypto";
import type { Stripe } from "./config.js";
import type { Sql } from "./db.js";
import type { User } from "./auth.js";

const API = "https://api.stripe.com/v1";

/** The events the webhook endpoint in Stripe has to send. */
export const EVENTS = [
  "checkout.session.completed",
  "customer.subscription.created",
  "customer.subscription.updated",
  "customer.subscription.deleted",
];

/** A subscription in one of these keeps the user's nodes supervised. */
const LIVE = new Set(["active", "trialing", "past_due"]);

export interface Subscription {
  status: string;
  currentPeriodEnd: Date | null;
  cancelAtPeriodEnd: boolean;
}

export function isLive(sub: Subscription | null): boolean {
  return sub !== null && LIVE.has(sub.status);
}

type Params = Record<string, string | number | boolean>;

async function call<T>(stripe: Stripe, method: "GET" | "POST", path: string, params: Params = {}): Promise<T> {
  const body = new URLSearchParams(Object.entries(params).map(([k, v]) => [k, String(v)]));
  const res = await fetch(`${API}${path}${method === "GET" && body.size ? `?${body}` : ""}`, {
    method,
    headers: {
      authorization: `Bearer ${stripe.secretKey}`,
      "content-type": "application/x-www-form-urlencoded",
    },
    body: method === "POST" ? body : undefined,
    signal: AbortSignal.timeout(15_000),
  });
  const json = (await res.json()) as T & { error?: { message?: string } };
  if (!res.ok) throw new Error(`Stripe ${method} ${path}: ${res.status} ${json.error?.message ?? ""}`.trim());
  return json;
}

export async function subscription(sql: Sql, userId: number): Promise<Subscription | null> {
  const [row] = await sql<{ status: string; current_period_end: Date | null; cancel_at_period_end: boolean }[]>`
    SELECT status, current_period_end, cancel_at_period_end FROM subscriptions
    WHERE user_id = ${userId} AND stripe_subscription_id IS NOT NULL`;
  return row
    ? { status: row.status, currentPeriodEnd: row.current_period_end, cancelAtPeriodEnd: row.cancel_at_period_end }
    : null;
}

/** The user's Stripe customer, made the first time they subscribe. */
async function customer(sql: Sql, stripe: Stripe, user: User): Promise<string> {
  const find = async () =>
    (await sql<{ stripe_customer_id: string }[]>`
      SELECT stripe_customer_id FROM subscriptions WHERE user_id = ${user.id}`)[0]?.stripe_customer_id;
  const known = await find();
  if (known) return known;
  const made = await call<{ id: string }>(stripe, "POST", "/customers", {
    email: user.email,
    "metadata[user_id]": user.id,
  });
  await sql`
    INSERT INTO subscriptions (user_id, stripe_customer_id) VALUES (${user.id}, ${made.id})
    ON CONFLICT (user_id) DO NOTHING`;
  // Two clicks at once: the first customer saved wins.
  return (await find()) ?? made.id;
}

/** Where to send the user to pay for the subscription. */
export async function checkoutUrl(sql: Sql, stripe: Stripe, publicUrl: string, user: User): Promise<string> {
  const session = await call<{ url: string }>(stripe, "POST", "/checkout/sessions", {
    mode: "subscription",
    customer: await customer(sql, stripe, user),
    client_reference_id: user.id,
    "line_items[0][price]": stripe.priceId,
    "line_items[0][quantity]": 1,
    allow_promotion_codes: true,
    success_url: `${publicUrl}/account?subscribed=1`,
    cancel_url: `${publicUrl}/pricing`,
  });
  return session.url;
}

/** Where the user manages their card and cancels, or null before they have subscribed. */
export async function portalUrl(sql: Sql, stripe: Stripe, publicUrl: string, user: User): Promise<string | null> {
  const [row] = await sql<{ stripe_customer_id: string }[]>`
    SELECT stripe_customer_id FROM subscriptions WHERE user_id = ${user.id}`;
  if (!row) return null;
  const session = await call<{ url: string }>(stripe, "POST", "/billing_portal/sessions", {
    customer: row.stripe_customer_id,
    return_url: `${publicUrl}/account`,
  });
  return session.url;
}

/**
 * Whether `header` (Stripe-Signature) signs `payload` with `secret`, within
 * `tolerance` seconds of `now`.
 */
export function verifySignature(payload: string, header: string, secret: string, now = Date.now() / 1000, tolerance = 300): boolean {
  const parts = header.split(",").map((p) => p.split("=", 2) as [string, string?]);
  const t = Number(parts.find(([k]) => k === "t")?.[1]);
  if (!Number.isFinite(t) || Math.abs(now - t) > tolerance) return false;
  const want = createHmac("sha256", secret).update(`${t}.${payload}`).digest();
  return parts
    .filter(([k, v]) => k === "v1" && v)
    .some(([, v]) => {
      const got = Buffer.from(v!, "hex");
      return got.length === want.length && timingSafeEqual(got, want);
    });
}

interface StripeSubscription {
  id: string;
  customer: string;
  status: string;
  cancel_at_period_end: boolean;
  current_period_end?: number;
  items?: { data?: { current_period_end?: number }[] };
}

export interface Event {
  id: string;
  type: string;
  data: { object: { subscription?: string | null; id?: string } };
}

/** The subscription an event is about, if any. */
function subscriptionId(event: Event): string | null {
  if (event.type === "checkout.session.completed") return event.data.object.subscription ?? null;
  if (event.type.startsWith("customer.subscription.")) return event.data.object.id ?? null;
  return null;
}

/**
 * Copies what a webhook event changed. The subscription is read back from
 * Stripe rather than taken from the event, so events arriving late or out
 * of order still leave its latest state. Each event is handled once.
 */
export async function handleEvent(sql: Sql, stripe: Stripe, event: Event): Promise<void> {
  const id = subscriptionId(event);
  const sub = id ? await call<StripeSubscription>(stripe, "GET", `/subscriptions/${encodeURIComponent(id)}`) : null;
  await sql.begin(async (tx) => {
    const fresh = await tx`INSERT INTO stripe_events (id) VALUES (${event.id}) ON CONFLICT DO NOTHING RETURNING id`;
    if (!fresh.length || !sub) return;
    const end = sub.items?.data?.[0]?.current_period_end ?? sub.current_period_end;
    await tx`
      UPDATE subscriptions SET
        stripe_subscription_id = ${sub.id},
        status = ${sub.status},
        current_period_end = ${end ? new Date(end * 1000) : null},
        cancel_at_period_end = ${sub.cancel_at_period_end},
        updated_at = now()
      WHERE stripe_customer_id = ${sub.customer}`;
  });
}
