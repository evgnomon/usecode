// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The code mailed to finish a sign up or a log in, after the password.

import { createHash, randomBytes, randomInt, timingSafeEqual } from "node:crypto";
import type { Sql } from "./db.js";

export type Purpose = "signup" | "login" | "reset";

/** How long a code works. */
export const CODE_MINUTES = 10;
/** Wrong codes a challenge takes before it's gone. */
export const MAX_TRIES = 5;

/** Six digits, from the OS random source, leading zeros kept. */
export function newCode(): string {
  return randomInt(0, 1_000_000).toString().padStart(6, "0");
}

/** Strips the spaces and dashes people type or paste a code with. */
export function cleanCode(code: string): string {
  return code.replace(/[\s-]/g, "");
}

const sha256 = (s: string) => createHash("sha256").update(s).digest();
const codeHash = (token: string, code: string) => sha256(`${token}\0${code}`);

export interface Pending {
  purpose: Purpose;
  email: string;
  /** Who logs in or resets; a sign up has no user yet. */
  userId: number | null;
  /** The new password's hash, for a sign up or a reset. */
  passwordHash: string | null;
}

/** A new challenge; the token goes back to the browser, the code into the mail. */
export async function start(sql: Sql, p: Pending): Promise<{ token: string; code: string }> {
  const token = randomBytes(32).toString("base64url");
  const code = newCode();
  await sql`
    INSERT INTO email_codes (token_hash, purpose, email, user_id, password_hash, code_hash, expires_at)
    VALUES (${sha256(token)}, ${p.purpose}, ${p.email}, ${p.userId}, ${p.passwordHash},
            ${codeHash(token, code)}, now() + make_interval(mins => ${CODE_MINUTES}))`;
  if (Math.random() < 0.05) await sql`DELETE FROM email_codes WHERE expires_at < now()`;
  return { token, code };
}

export type Checked = { ok: true; pending: Pending } | { ok: false; why: "gone" | "wrong" };

/**
 * Checks `code` against the challenge `token` names. A right code uses the
 * challenge up; each wrong one counts, and the last allowed one ends it.
 */
export async function check(sql: Sql, token: string, code: string): Promise<Checked> {
  if (!token) return { ok: false, why: "gone" };
  const key = sha256(token);
  const [row] = await sql<
    { purpose: Purpose; email: string; user_id: string | null; password_hash: string | null; code_hash: Buffer; attempts: number }[]
  >`
    UPDATE email_codes SET attempts = attempts + 1
    WHERE token_hash = ${key} AND expires_at > now() AND attempts < ${MAX_TRIES}
    RETURNING purpose, email, user_id, password_hash, code_hash, attempts`;
  if (!row) return { ok: false, why: "gone" };
  const want = Buffer.from(row.code_hash);
  const got = codeHash(token, cleanCode(code));
  if (got.length !== want.length || !timingSafeEqual(got, want)) {
    if (row.attempts >= MAX_TRIES) await sql`DELETE FROM email_codes WHERE token_hash = ${key}`;
    return { ok: false, why: row.attempts >= MAX_TRIES ? "gone" : "wrong" };
  }
  // A reset for nobody: its code was never mailed. Should it be guessed
  // anyway, it still only runs out.
  if (row.purpose === "reset" && row.user_id === null) return { ok: false, why: row.attempts >= MAX_TRIES ? "gone" : "wrong" };
  // Only one request gets to use it, however many arrive together.
  const used = await sql`DELETE FROM email_codes WHERE token_hash = ${key} RETURNING 1`;
  if (!used.length) return { ok: false, why: "gone" };
  return {
    ok: true,
    pending: {
      purpose: row.purpose,
      email: row.email,
      userId: row.user_id === null ? null : Number(row.user_id),
      passwordHash: row.password_hash,
    },
  };
}

/** The mail a code goes out in. */
export function message(purpose: Purpose, code: string): { subject: string; text: string } {
  const what = { signup: "finish signing up to", login: "log in to", reset: "set a new password on" }[purpose];
  const ignore = {
    signup: "If you didn't sign up, someone typed your address by mistake. Ignore this mail and no account is made.",
    login: "If this wasn't you, someone knows your password. Don't share this code with anyone.",
    reset: "If you didn't ask for this, ignore this mail: your password stays as it is. Don't share this code with anyone.",
  }[purpose];
  return {
    subject: `${code} is your usecode.dev code`,
    text: `Hi,

Your code to ${what} usecode.dev is:

    ${code}

It works for ${CODE_MINUTES} minutes.

${ignore}

usecode.dev
`,
  };
}
