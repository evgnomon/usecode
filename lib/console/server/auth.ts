// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { createHash, randomBytes, scrypt, timingSafeEqual } from "node:crypto";
import type { Sql } from "./db.js";

export const SESSION_COOKIE = "uc_session";
export const SESSION_DAYS = 30;

const SCRYPT = { N: 1 << 15, r: 8, p: 1, maxmem: 64 * 1024 * 1024 };
const KEY_LEN = 32;

function derive(password: string, salt: Buffer, opts: { N: number; r: number; p: number }): Promise<Buffer> {
  return new Promise((resolve, reject) =>
    scrypt(password.normalize("NFKC"), salt, KEY_LEN, { ...opts, maxmem: SCRYPT.maxmem }, (err, key) =>
      err ? reject(err) : resolve(key),
    ),
  );
}

/** `scrypt$N$r$p$salt$hash`, base64url, so the cost can rise later and old hashes still check. */
export async function hashPassword(password: string): Promise<string> {
  const salt = randomBytes(16);
  const key = await derive(password, salt, SCRYPT);
  return ["scrypt", SCRYPT.N, SCRYPT.r, SCRYPT.p, salt.toString("base64url"), key.toString("base64url")].join("$");
}

export async function verifyPassword(password: string, stored: string): Promise<boolean> {
  const [scheme, n, r, p, salt, hash] = stored.split("$");
  if (scheme !== "scrypt" || !n || !r || !p || !salt || !hash) return false;
  const want = Buffer.from(hash, "base64url");
  const got = await derive(password, Buffer.from(salt, "base64url"), { N: Number(n), r: Number(r), p: Number(p) });
  return got.length === want.length && timingSafeEqual(got, want);
}

/** Checked against when the email is unknown, so a login takes as long either way. */
let dummyHash: Promise<string> | undefined;
function dummy(): Promise<string> {
  dummyHash ??= hashPassword(randomBytes(16).toString("hex"));
  return dummyHash;
}

export function normalizeEmail(email: string): string {
  return email.trim().toLowerCase();
}

/** Why `email` and `password` can't make an account, or null when they can. */
export function signupProblem(email: string, password: string): string | null {
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) || email.length > 254) {
    return "That doesn't look like an email address.";
  }
  if (password.length < 10) return "Pick a password of at least 10 characters.";
  if (password.length > 1024) return "That password is a bit too long.";
  return null;
}

export interface User {
  id: number;
  email: string;
}

/** The new user, or null when the email already has an account. */
export async function createUser(sql: Sql, email: string, password: string): Promise<User | null> {
  const hash = await hashPassword(password);
  const rows = await sql<{ id: string; email: string }[]>`
    INSERT INTO users (email, password_hash) VALUES (${email}, ${hash})
    ON CONFLICT (email) DO NOTHING
    RETURNING id, email`;
  const row = rows[0];
  return row ? { id: Number(row.id), email: row.email } : null;
}

export async function checkLogin(sql: Sql, email: string, password: string): Promise<User | null> {
  const [row] = await sql<{ id: string; email: string; password_hash: string }[]>`
    SELECT id, email, password_hash FROM users WHERE email = ${email}`;
  if (!row) {
    await verifyPassword(password, await dummy());
    return null;
  }
  return (await verifyPassword(password, row.password_hash)) ? { id: Number(row.id), email: row.email } : null;
}

function tokenHash(token: string): Buffer {
  return createHash("sha256").update(token).digest();
}

/** A new session for `userId`; the returned token goes in the cookie. */
export async function startSession(sql: Sql, userId: number): Promise<string> {
  const token = randomBytes(32).toString("base64url");
  await sql`
    INSERT INTO sessions (token_hash, user_id, expires_at)
    VALUES (${tokenHash(token)}, ${userId}, now() + make_interval(days => ${SESSION_DAYS}))`;
  // Clearing out old sessions now and then keeps the table small.
  if (Math.random() < 0.05) await sql`DELETE FROM sessions WHERE expires_at < now()`;
  return token;
}

export async function sessionUser(sql: Sql, token: string | undefined): Promise<User | null> {
  if (!token) return null;
  const [row] = await sql<{ id: string; email: string }[]>`
    SELECT u.id, u.email FROM sessions s JOIN users u ON u.id = s.user_id
    WHERE s.token_hash = ${tokenHash(token)} AND s.expires_at > now()`;
  return row ? { id: Number(row.id), email: row.email } : null;
}

export async function endSession(sql: Sql, token: string | undefined): Promise<void> {
  if (token) await sql`DELETE FROM sessions WHERE token_hash = ${tokenHash(token)}`;
}
