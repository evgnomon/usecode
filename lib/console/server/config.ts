// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

/** Stripe, when billing is switched on. */
export interface Stripe {
  secretKey: string;
  priceId: string;
  webhookSecret: string;
}

export interface Config {
  port: number;
  /** Where the console is reached, e.g. https://usecode.dev; links and Stripe redirects use it. */
  publicUrl: string;
  /** A postgres:// URL; when empty the PG* variables (PGHOST, PGUSER, ...) are used. */
  databaseUrl: string;
  /** Session cookies only travel over HTTPS. */
  secureCookies: boolean;
  stripe: Stripe | null;
}

export function load(env: NodeJS.ProcessEnv = process.env): Config {
  const publicUrl = (env.PUBLIC_URL ?? "http://localhost:8080").replace(/\/+$/, "");
  const secretKey = env.STRIPE_SECRET_KEY ?? "";
  const priceId = env.STRIPE_PRICE_ID ?? "";
  const webhookSecret = env.STRIPE_WEBHOOK_SECRET ?? "";
  return {
    port: Number(env.PORT ?? 8080),
    publicUrl,
    databaseUrl: env.DATABASE_URL ?? "",
    secureCookies: publicUrl.startsWith("https://"),
    stripe: secretKey && priceId && webhookSecret ? { secretKey, priceId, webhookSecret } : null,
  };
}
