// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { serve } from "@hono/node-server";
import { fileURLToPath } from "node:url";
import { createApp } from "./app.js";
import * as config from "./config.js";
import { connect, migrate } from "./db.js";

// dist/server/main.js, two levels below the package.
const root = fileURLToPath(new URL("../../", import.meta.url));

const cfg = config.load();
const sql = connect(cfg.databaseUrl);

// Postgres may still be starting next to us; keep trying rather than
// crash-looping.
for (let attempt = 1; ; attempt++) {
  try {
    const applied = await migrate(sql, `${root}migrations`);
    if (applied.length) console.log(`applied migrations: ${applied.join(", ")}`);
    break;
  } catch (err) {
    if (attempt >= 30) throw err;
    console.log(`waiting for postgres (${(err as Error).message})`);
    await new Promise((r) => setTimeout(r, 2000));
  }
}
if (!cfg.stripe) console.log("billing is off: set STRIPE_SECRET_KEY, STRIPE_PRICE_ID and STRIPE_WEBHOOK_SECRET");

const server = serve({ fetch: createApp(sql, cfg, `${root}dist/client`).fetch, port: cfg.port }, (info) =>
  console.log(`console on :${info.port}, public at ${cfg.publicUrl}`),
);

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => {
    server.close();
    void sql.end({ timeout: 5 }).then(() => process.exit(0));
  });
}
