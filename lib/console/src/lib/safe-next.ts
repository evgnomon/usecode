// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

/** `next` when it is a path on this site, so a login can't send anyone elsewhere. */
export function safeNext(next: unknown, fallback = "/account"): string {
  return typeof next === "string" && /^\/(?![/\\])/.test(next) ? next : fallback;
}
