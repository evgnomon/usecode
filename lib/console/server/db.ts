// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import postgres from "postgres";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

export type Sql = postgres.Sql;

export function connect(url: string): Sql {
  const options = { max: 10, onnotice: () => {} };
  return url ? postgres(url, options) : postgres(options);
}

/** Any number fits: the console takes it for itself while migrating. */
const MIGRATE_LOCK = 0x75636f6e;

/**
 * Applies the migrations/*.sql not applied yet, in name order, each in its
 * own transaction. Replicas starting together take turns on an advisory
 * lock, so each file runs once.
 */
export async function migrate(sql: Sql, dir: string): Promise<string[]> {
  const files = (await readdir(dir)).filter((f) => f.endsWith(".sql")).sort();
  const applied: string[] = [];
  await sql.begin(async (tx) => {
    await tx`SELECT pg_advisory_xact_lock(${MIGRATE_LOCK})`;
    await tx`CREATE TABLE IF NOT EXISTS schema_migrations (
      name text PRIMARY KEY,
      applied_at timestamptz NOT NULL DEFAULT now()
    )`;
    const done = new Set((await tx`SELECT name FROM schema_migrations`).map((r) => r.name as string));
    for (const file of files) {
      if (done.has(file)) continue;
      await tx.unsafe(await readFile(join(dir, file), "utf8"));
      await tx`INSERT INTO schema_migrations (name) VALUES (${file})`;
      applied.push(file);
    }
  });
  return applied;
}
