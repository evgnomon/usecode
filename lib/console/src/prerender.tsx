// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

/// <reference types="node" />

// Renders the content pages into HTML at build time, so search engines, AI
// tools and anyone without JavaScript can read them. The browser hydrates
// them and carries on as usual. Who's logged in isn't known here, so those
// bits fill in once /api/me answers, just like on the other pages.
//
// Run after `vite build`: node dist/prerender/prerender.js dist/client

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { StrictMode } from "react";
import { renderToString } from "react-dom/server";
import { App, NOT_FOUND, PAGES, type Page } from "@/App";
import { MeProvider } from "@/lib/api";
import { StaticLocation } from "@/lib/router";

const ROOT = '<div id="root"></div>';
const TITLE = /<title>[^<]*<\/title>/;

const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** Where server/app.ts looks for a page: pages/index.html, pages/pricing.html, ... */
const fileFor = (path: string) => `pages/${path === "/" ? "index" : path.slice(1)}.html`;

const client = process.argv[2] ?? "dist/client";
const shell = await readFile(join(client, "index.html"), "utf8");
if (!shell.includes(ROOT) || !TITLE.test(shell)) throw new Error(`index.html has no ${ROOT} or <title>`);

const pages: [string, Page, string][] = [
  ...Object.entries(PAGES).map(([path, page]): [string, Page, string] => [path, page, fileFor(path)]),
  ["/404", NOT_FOUND, "pages/404.html"],
];

await mkdir(join(client, "pages"), { recursive: true });
for (const [path, page, file] of pages.filter(([, page]) => page.prerender)) {
  const html = renderToString(
    <StrictMode>
      <MeProvider>
        <StaticLocation href={path}>
          <App />
        </StaticLocation>
      </MeProvider>
    </StrictMode>,
  );
  // server/app.ts's CSP blocks style attributes that come in with the HTML.
  if (/<[^>]+\sstyle="/.test(html)) throw new Error(`${path} has an inline style the CSP would block`);
  // Functions, so a "$" in the page (there are prices) isn't read as a pattern.
  const out = shell
    .replace(TITLE, () => `<title>${escape(page.title)}</title>`)
    .replace(ROOT, () => `<div id="root">${html}</div>`);
  await writeFile(join(client, file), out);
}
