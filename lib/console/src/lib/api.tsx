// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Talking to the API (lib/api), straight from the browser. Its address is
// in the page (server/app.ts puts it there); the session is its cookie.

import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";

/** GET /account, as the API sends it. */
export interface Me {
  user: { email: string } | null;
  billing: boolean;
  subscription: { status: string; currentPeriodEnd: string | null; cancelAtPeriodEnd: boolean } | null;
  subscribed: boolean;
}

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
  }
}

const ON_OUR_SIDE = "Something went wrong on our side. Please try again in a moment.";

/** Where the API is: the page says, else the one `make -C lib/api run` starts. */
export function apiUrl(): string {
  if (typeof document === "undefined") return "";
  return document.querySelector<HTMLMetaElement>('meta[name="uc-api"]')?.content || "http://localhost:8000";
}

/** POSTs `body` as JSON to the API; the reply's `detail` becomes an ApiError. */
export async function post<T = unknown>(path: string, body: unknown = {}): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`${apiUrl()}${path}`, {
      method: "POST",
      credentials: "include",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch {
    throw new ApiError("We can't reach usecode.dev just now. Check your connection and try again.", 0);
  }
  const json = res.status === 204 ? {} : await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(typeof json.detail === "string" ? json.detail : ON_OUR_SIDE, res.status);
  return json as T;
}

/** Sends the browser to Stripe (or back to the account) with the URL an endpoint answers. */
export async function goTo(path: "/billing/checkout" | "/billing/portal") {
  const { url } = await post<{ url: string }>(path);
  window.location.assign(url);
}

const MeContext = createContext<{ me: Me | null; refresh: () => Promise<void> }>({
  me: null,
  refresh: async () => {},
});

export function MeProvider({ children }: { children: ReactNode }) {
  const [me, setMe] = useState<Me | null>(null);
  const refresh = useCallback(async () => {
    const res = await fetch(`${apiUrl()}/account`, { credentials: "include" }).catch(() => null);
    if (res?.ok) setMe((await res.json()) as Me);
  }, []);
  useEffect(() => void refresh(), [refresh]);
  return <MeContext.Provider value={{ me, refresh }}>{children}</MeContext.Provider>;
}

/** Who's looking, null until the first answer. */
export const useMe = () => useContext(MeContext);
