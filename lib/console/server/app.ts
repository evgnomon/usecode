// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The console's HTTP side: the pages, as Vite built and prerendered them,
// sent to the right one for whoever is looking. It keeps no data: accounts,
// billing and nodes are the API's (lib/api), which the pages call straight
// from the browser. This server only asks the API who is logged in, to
// send them where they belong before a page loads.

import { Hono, type Context } from "hono";
import { serveStatic } from "@hono/node-server/serve-static";
import { HTTPException } from "hono/http-exception";
import { secureHeaders } from "hono/secure-headers";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import type { Config } from "./config.js";

/** The pages the app in src/ has; any other path gets it with a 404. */
export const PAGES = ["/", "/pricing", "/login", "/signup", "/forgot", "/account", "/hosts"];

/** Pages for the logged in, and the ones for the logged out. */
const MEMBERS = ["/account", "/hosts"];
const VISITORS = ["/login", "/signup", "/forgot"];

/** The <meta> the pages read the API's address from (src/lib/api.tsx). */
export const API_META = "uc-api";

/** What the server calls the API with; tests hand in their own. */
export type Fetch = (url: string, init?: RequestInit) => Promise<Response>;

/**
 * Whether the browser sending `cookie` is logged in, by asking the API:
 * null when it can't say, and then nobody is sent anywhere.
 */
export async function loggedIn(config: Config, cookie: string | undefined, fetchApi: Fetch): Promise<boolean | null> {
  if (!cookie?.includes("uc_session=")) return false;
  try {
    const res = await fetchApi(`${config.apiInternalUrl}/account`, {
      headers: { cookie },
      signal: AbortSignal.timeout(3000),
    });
    if (!res.ok) return null;
    const me = (await res.json()) as { user: unknown };
    return me.user !== null;
  } catch {
    return null;
  }
}

/** The page's HTML with the API's address in it. */
export function withApi(html: string, apiUrl: string): string {
  const escaped = apiUrl.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
  return html.replace("<head>", `<head>\n    <meta name="${API_META}" content="${escaped}" />`);
}

/**
 * `clientDir` holds the built pages: index.html, assets/, public/'s files,
 * and pages/ with the ones src/prerender.tsx rendered ahead of time.
 */
export function createApp(config: Config, clientDir: string, fetchApi: Fetch = fetch): Hono {
  const app = new Hono();
  const files = new Map<string, Promise<string>>();
  const file = (name: string) => {
    if (!files.has(name)) files.set(name, readFile(join(clientDir, name), "utf8").then((html) => withApi(html, config.apiUrl)));
    return files.get(name)!;
  };

  app.get("/healthz", (c) => c.text("ok"));

  // Stripe's webhook used to come here; the API takes it now, at
  // https://api.<host>/stripe/webhook. Until Stripe is pointed there, this
  // hands it on, raw body and signature as they are.
  app.post("/stripe/webhook", async (c) => {
    const res = await fetchApi(`${config.apiInternalUrl}/stripe/webhook`, {
      method: "POST",
      headers: {
        "content-type": c.req.header("content-type") ?? "application/json",
        "stripe-signature": c.req.header("stripe-signature") ?? "",
      },
      body: await c.req.text(),
    });
    return c.body(await res.text(), res.status as 200, { "content-type": "application/json" });
  });

  app.use(
    secureHeaders({
      contentSecurityPolicy: {
        defaultSrc: ["'self'"],
        connectSrc: ["'self'", new URL(config.apiUrl).origin],
        formAction: ["'self'"],
        frameAncestors: ["'none'"],
      },
    }),
  );

  // The pages. Vite names its bundles by their content, so they keep.
  app.get("/assets/*", serveStatic({ root: clientDir, onFound: (_, c) => c.header("cache-control", "public, max-age=31536000, immutable") }));

  const sendTo = (where: string, when: boolean) => async (c: Context, next: () => Promise<void>) => {
    if ((await loggedIn(config, c.req.header("cookie"), fetchApi)) === when) return c.redirect(where);
    await next();
  };
  for (const path of MEMBERS) app.get(path, sendTo(`/login?next=${path}`, false));
  for (const path of VISITORS) app.get(path, sendTo("/account", true));

  // A prerendered page if there is one, else the app renders it in the browser.
  const page = async (c: Context) => {
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
    return c.text("Something went wrong on our side. Please try again in a moment.", 500);
  });

  return app;
}
