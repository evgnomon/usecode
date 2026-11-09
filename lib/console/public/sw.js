// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The service worker of the installed app. Everything comes from the
// network as before, nothing is cached; it only shows a page of its own
// when a page can't be loaded because the network is gone.

const OFFLINE = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>usecode.dev, offline</title>
<style>
  html { color-scheme: light dark; font-family: system-ui, sans-serif; }
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; text-align: center; }
  img { width: 64px; height: 64px; }
  a { color: inherit; }
</style>
</head>
<body>
<main>
  <img src="/icon.svg" alt="">
  <h1>You're offline</h1>
  <p>usecode.dev needs the network. <a href="">Try again</a> once you're back.</p>
</main>
</body>
</html>`;

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open("offline").then((cache) => cache.add("/icon.svg")).then(() => self.skipWaiting()));
});

self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.mode === "navigate") {
    event.respondWith(
      fetch(request).catch(() => new Response(OFFLINE, { headers: { "content-type": "text/html; charset=utf-8" } })),
    );
  } else if (new URL(request.url).pathname === "/icon.svg") {
    event.respondWith(fetch(request).catch(() => caches.match("/icon.svg")));
  }
});
