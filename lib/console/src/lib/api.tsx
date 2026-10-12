// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Talking to the console's own API (server/app.ts).

import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";

/** GET /api/me, as server/app.ts sends it. */
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

/** POSTs `body` as JSON; the reply's `error` becomes an ApiError. */
export async function post<T = unknown>(path: string, body: unknown = {}): Promise<T> {
  const res = await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  const json = res.status === 204 ? {} : await res.json().catch(() => ({}));
  if (!res.ok) throw new ApiError(json.error ?? "Something went wrong on our side. Please try again in a moment.", res.status);
  return json as T;
}

/** Sends the browser to Stripe (or back to the account) with the URL an endpoint answers. */
export async function goTo(path: "/api/billing/checkout" | "/api/billing/portal") {
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
    const res = await fetch("/api/me");
    if (res.ok) setMe((await res.json()) as Me);
  }, []);
  useEffect(() => void refresh(), [refresh]);
  return <MeContext.Provider value={{ me, refresh }}>{children}</MeContext.Provider>;
}

/** Who's looking, null until the first answer. */
export const useMe = () => useContext(MeContext);
