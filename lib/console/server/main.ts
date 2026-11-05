// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { serve } from "@hono/node-server";
import { fileURLToPath } from "node:url";
import { createApp } from "./app.js";
import * as config from "./config.js";

// dist/server/main.js, two levels below the package.
const root = fileURLToPath(new URL("../../", import.meta.url));

const cfg = config.load();

const server = serve({ fetch: createApp(cfg, `${root}dist/client`).fetch, port: cfg.port }, (info) =>
  console.log(`console on :${info.port}, public at ${cfg.publicUrl}, API at ${cfg.apiUrl}`),
);

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => server.close(() => process.exit(0)));
}
