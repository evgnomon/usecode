// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

/**
 * `next` when it is a path on this site, so a login can't send anyone
 * elsewhere; or, given the API's address, its login for another site
 * (`<api>/oauth/authorize?...`), which only sends the browser on to an
 * address that site has with the API.
 */
export function safeNext(next: unknown, fallback = "/account", api = ""): string {
  if (typeof next !== "string") return fallback;
  if (/^\/(?![/\\])/.test(next)) return next;
  if (/^https?:\/\/[^/?#]+$/.test(api) && next.startsWith(`${api}/oauth/authorize?`)) return next;
  return fallback;
}
