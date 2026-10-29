// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The console's HTTP side: a small JSON API under /api for the pages, the
// Stripe webhook, and the pages themselves, as Vite built them.

import { Hono, type Context } from "hono";
import { serveStatic } from "@hono/node-server/serve-static";
import { csrf } from "hono/csrf";
import { HTTPException } from "hono/http-exception";
import { secureHeaders } from "hono/secure-headers";
import { deleteCookie, getCookie, setCookie } from "hono/cookie";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import * as auth from "./auth.js";
import * as billing from "./billing.js";
import * as mail from "./mail.js";
import * as otp from "./otp.js";
import type { Config } from "./config.js";
import type { Sql } from "./db.js";

type Env = { Variables: { user: auth.User | null } };

/** The pages the app in src/ has; any other path gets it with a 404. */
export const PAGES = ["/", "/pricing", "/login", "/signup", "/forgot", "/account"];

/** The cookie holding the token of the code waiting to be typed in. */
export const OTP_COOKIE = "uc_otp";

/** Failed logins per email, so a password can't be guessed at speed. */
export class Limiter {
  private hits = new Map<string, number[]>();
  constructor(
    private max = 10,
    private windowMs = 15 * 60_000,
  ) {}
  blocked(key: string, now = Date.now()): boolean {
    const recent = (this.hits.get(key) ?? []).filter((t) => now - t < this.windowMs);
    if (recent.length) this.hits.set(key, recent);
    else this.hits.delete(key);
    return recent.length >= this.max;
  }
  fail(key: string, now = Date.now()): void {
    this.hits.set(key, [...(this.hits.get(key) ?? []), now]);
  }
  clear(key: string): void {
    this.hits.delete(key);
  }
}

/** What the pages know about whoever is looking: GET /api/me. */
export interface Me {
  user: { email: string } | null;
  /** Whether subscribing is open, i.e. Stripe is set up. */
  billing: boolean;
  subscription: { status: string; currentPeriodEnd: string | null; cancelAtPeriodEnd: boolean } | null;
  subscribed: boolean;
}

/** The email and password a signup or login posted. */
async function credentials(c: Context<Env>): Promise<{ email: string; password: string }> {
  const body = (await c.req.json().catch(() => ({}))) as Record<string, unknown>;
  return { email: auth.normalizeEmail(String(body.email ?? "")), password: String(body.password ?? "") };
}

/**
 * `clientDir` holds the built pages: index.html, assets/, public/'s files,
 * and pages/ with the ones src/prerender.tsx rendered ahead of time.
 */
export function createApp(sql: Sql, config: Config, clientDir: string, mailer: mail.Mailer = mail.fromConfig(config)): Hono<Env> {
  const app = new Hono<Env>();
  const limiter = new Limiter();
  // Codes mailed per address, so nobody's inbox gets flooded.
  const mailed = new Limiter(5, 15 * 60_000);
  const files = new Map<string, Promise<string>>();
  const file = (name: string) => {
    if (!files.has(name)) files.set(name, readFile(join(clientDir, name), "utf8"));
    return files.get(name)!;
  };

  app.get("/healthz", async (c) => {
    await sql`SELECT 1`;
    return c.text("ok");
  });

  // Stripe signs the raw body; it comes from Stripe, not a browser, so the
  // checks below never see it.
  app.post("/stripe/webhook", async (c) => {
    if (!config.stripe) return c.text("billing is off", 404);
    const payload = await c.req.text();
    if (!billing.verifySignature(payload, c.req.header("stripe-signature") ?? "", config.stripe.webhookSecret)) {
      return c.text("bad signature", 400);
    }
    await billing.handleEvent(sql, config.stripe, JSON.parse(payload) as billing.Event);
    return c.json({ received: true });
  });

  app.use(
    secureHeaders({
      contentSecurityPolicy: {
        defaultSrc: ["'self'"],
        formAction: ["'self'"],
        frameAncestors: ["'none'"],
      },
    }),
  );
  app.use(csrf({ origin: config.publicUrl }));

  // Only JSON is posted to the API, which a page on another site can't send
  // without asking first.
  app.use("/api/*", async (c, next) => {
    if (c.req.method === "POST" && !c.req.header("content-type")?.startsWith("application/json")) {
      return c.json({ error: "Send JSON." }, 415);
    }
    await next();
  });

  app.use(async (c, next) => {
    c.set("user", await auth.sessionUser(sql, getCookie(c, auth.SESSION_COOKIE)));
    await next();
  });

  const login = (c: Context<Env>, token: string) =>
    setCookie(c, auth.SESSION_COOKIE, token, {
      httpOnly: true,
      secure: config.secureCookies,
      sameSite: "Lax",
      path: "/",
      maxAge: auth.SESSION_DAYS * 24 * 3600,
    });

  app.get("/api/me", async (c) => {
    const user = c.var.user;
    const sub = user ? await billing.subscription(sql, user.id) : null;
    return c.json<Me>({
      user: user && { email: user.email },
      billing: config.stripe !== null,
      subscription: sub && {
        status: sub.status,
        currentPeriodEnd: sub.currentPeriodEnd?.toISOString() ?? null,
        cancelAtPeriodEnd: sub.cancelAtPeriodEnd,
      },
      subscribed: billing.isLive(sub),
    });
  });

  const tooMany = (c: Context<Env>) =>
    c.json({ error: "Too many tries. Take a break and try again in a few minutes." }, 429);

  // Mails a code for `pending` and keeps its token in a cookie; the browser
  // then posts the code to /api/verify.
  const challenge = async (c: Context<Env>, pending: otp.Pending) => {
    if (mailed.blocked(pending.email)) {
      return c.json({ error: "We've sent a few codes already. Check your inbox, or try again in a few minutes." }, 429);
    }
    const { token, code } = await otp.start(sql, pending);
    const { subject, text } = otp.message(pending.purpose, code);
    // A reset for an address without an account: nothing to mail.
    const nobody = pending.purpose === "reset" && pending.userId === null;
    try {
      if (!nobody) await mailer.send(pending.email, subject, text);
    } catch (err) {
      console.error(`mailing a code to ${pending.email}: ${(err as Error).message}`);
      return c.json({ error: "We couldn't send you the code just now. Please try again in a moment." }, 503);
    }
    mailed.fail(pending.email);
    setCookie(c, OTP_COOKIE, token, {
      httpOnly: true,
      secure: config.secureCookies,
      sameSite: "Strict",
      path: "/api/verify",
      maxAge: otp.CODE_MINUTES * 60,
    });
    return c.json({ code: true, email: pending.email }, 202);
  };

  // Step one of each: the email and password, then a code goes out.
  app.post("/api/signup", async (c) => {
    const { email, password } = await credentials(c);
    const problem = auth.signupProblem(email, password);
    if (problem) return c.json({ error: problem }, 422);
    if (await auth.emailTaken(sql, email)) {
      return c.json({ error: "That email already has an account. Try logging in." }, 409);
    }
    const passwordHash = await auth.hashPassword(password);
    return challenge(c, { purpose: "signup", email, userId: null, passwordHash });
  });

  app.post("/api/login", async (c) => {
    const { email, password } = await credentials(c);
    if (limiter.blocked(email)) return tooMany(c);
    const user = await auth.checkLogin(sql, email, password);
    if (!user) {
      limiter.fail(email);
      return c.json({ error: "That email and password don't match." }, 401);
    }
    return challenge(c, { purpose: "login", email: user.email, userId: user.id, passwordHash: null });
  });

  // A forgotten password: the address and a new password, then a code goes
  // out. An unknown address gets the same answer and no mail, so this
  // doesn't say who has an account.
  app.post("/api/reset", async (c) => {
    const { email, password } = await credentials(c);
    const problem = auth.signupProblem(email, password);
    if (problem) return c.json({ error: problem }, 422);
    const passwordHash = await auth.hashPassword(password);
    const user = await auth.findUser(sql, email);
    return challenge(c, { purpose: "reset", email, userId: user?.id ?? null, passwordHash });
  });

  // Step two: the code from the mail.
  app.post("/api/verify", async (c) => {
    const body = (await c.req.json().catch(() => ({}))) as Record<string, unknown>;
    const checked = await otp.check(sql, getCookie(c, OTP_COOKIE) ?? "", String(body.code ?? ""));
    if (!checked.ok) {
      if (checked.why === "wrong") return c.json({ error: "That code isn't right. Check the mail and try again." }, 401);
      deleteCookie(c, OTP_COOKIE, { path: "/api/verify" });
      return c.json({ error: "That code has run out. Go back and we'll send you a new one." }, 410);
    }
    deleteCookie(c, OTP_COOKIE, { path: "/api/verify" });
    const p = checked.pending;
    let user: auth.User | null;
    if (p.purpose === "signup") {
      user = await auth.insertUser(sql, p.email, p.passwordHash!);
      if (!user) return c.json({ error: "That email already has an account. Try logging in." }, 409);
    } else {
      user = { id: p.userId!, email: p.email };
      if (p.purpose === "reset") await auth.setPassword(sql, user.id, p.passwordHash!);
      limiter.clear(p.email);
    }
    mailed.clear(p.email);
    login(c, await auth.startSession(sql, user.id));
    return c.json({ email: user.email }, p.purpose === "signup" ? 201 : 200);
  });

  app.post("/api/logout", async (c) => {
    await auth.endSession(sql, getCookie(c, auth.SESSION_COOKIE));
    deleteCookie(c, auth.SESSION_COOKIE, { path: "/" });
    return c.body(null, 204);
  });

  // Both answer with where to send the browser next.
  app.post("/api/billing/checkout", async (c) => {
    const user = c.var.user;
    if (!user) return c.json({ error: "Log in first." }, 401);
    if (!config.stripe) return c.json({ error: "Subscriptions aren't open yet." }, 409);
    if (billing.isLive(await billing.subscription(sql, user.id))) return c.json({ url: "/account" });
    return c.json({ url: await billing.checkoutUrl(sql, config.stripe, config.publicUrl, user) });
  });

  app.post("/api/billing/portal", async (c) => {
    const user = c.var.user;
    if (!user) return c.json({ error: "Log in first." }, 401);
    const url = config.stripe && (await billing.portalUrl(sql, config.stripe, config.publicUrl, user));
    return c.json({ url: url || "/account" });
  });

  app.all("/api/*", (c) => c.json({ error: "No such thing." }, 404));

  // The pages. Vite names its bundles by their content, so they keep.
  app.get("/assets/*", serveStatic({ root: clientDir, onFound: (_, c) => c.header("cache-control", "public, max-age=31536000, immutable") }));
  app.get("/account", async (c, next) => (c.var.user ? next() : c.redirect("/login?next=/account")));
  app.get("/login", async (c, next) => (c.var.user ? c.redirect("/account") : next()));
  app.get("/signup", async (c, next) => (c.var.user ? c.redirect("/account") : next()));
  app.get("/forgot", async (c, next) => (c.var.user ? c.redirect("/account") : next()));
  // A prerendered page if there is one, else the app renders it in the browser.
  const page = async (c: Context<Env>) => {
    const path = c.req.path;
    const known = PAGES.includes(path);
    const name = !known ? "404" : path === "/" ? "index" : path.slice(1);
    const html = await file(`pages/${name}.html`).catch(() => file("index.html"));
    c.header("cache-control", "no-cache");
    return c.html(html, known ? 200 : 404);
  };
  // A path with an extension is a file from public/, the rest are pages.
  app.get("*", async (c, next) => (/\.\w+$/.test(c.req.path) ? next() : page(c)));
  app.get("*", serveStatic({ root: clientDir, onFound: (_, c) => c.header("cache-control", "public, max-age=3600") }));
  app.get("*", page);

  app.onError((err, c) => {
    if (err instanceof HTTPException) return err.getResponse();
    console.error(err);
    return c.json({ error: "Something went wrong on our side. Please try again in a moment." }, 500);
  });

  return app;
}
