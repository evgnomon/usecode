// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

/** Stripe, when billing is switched on. */
export interface Stripe {
  secretKey: string;
  priceId: string;
  webhookSecret: string;
}

/** The SMTP server the login codes go out through. */
export interface Smtp {
  host: string;
  port: number;
  user: string;
  password: string;
  /** The From: of the mail, e.g. `usecode.dev <no-reply@usecode.dev>`. */
  from: string;
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
  /** Mail is on once SMTP_PASSWORD is set; the rest have defaults. */
  smtp: Smtp | null;
}

export function load(env: NodeJS.ProcessEnv = process.env): Config {
  const publicUrl = (env.PUBLIC_URL ?? "http://localhost:8080").replace(/\/+$/, "");
  const secretKey = env.STRIPE_SECRET_KEY ?? "";
  const priceId = env.STRIPE_PRICE_ID ?? "";
  const webhookSecret = env.STRIPE_WEBHOOK_SECRET ?? "";
  const smtpUser = env.SMTP_USER || "no-reply@usecode.dev";
  const smtpPassword = env.SMTP_PASSWORD ?? "";
  return {
    port: Number(env.PORT ?? 8080),
    publicUrl,
    databaseUrl: env.DATABASE_URL ?? "",
    secureCookies: publicUrl.startsWith("https://"),
    stripe: secretKey && priceId && webhookSecret ? { secretKey, priceId, webhookSecret } : null,
    smtp: smtpPassword
      ? {
          host: env.SMTP_HOST || "mail.clusterlean.net",
          port: Number(env.SMTP_PORT || 465),
          user: smtpUser,
          password: smtpPassword,
          from: env.SMTP_FROM || `usecode.dev <${smtpUser}>`,
        }
      : null,
  };
}
