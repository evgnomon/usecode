// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Just enough routing for a handful of pages: the address bar is the state.

import {
  createContext,
  useContext,
  useEffect,
  useSyncExternalStore,
  type ComponentProps,
  type MouseEvent,
  type ReactNode,
} from "react";

const listeners = new Set<() => void>();

function subscribe(listener: () => void) {
  listeners.add(listener);
  window.addEventListener("popstate", listener);
  return () => {
    listeners.delete(listener);
    window.removeEventListener("popstate", listener);
  };
}

export function navigate(to: string, { replace = false } = {}) {
  if (replace) history.replaceState(null, "", to);
  else history.pushState(null, "", to);
  window.scrollTo(0, 0);
  listeners.forEach((l) => l());
}

const current = () => location.pathname + location.search;

// Where the prerender (src/prerender.tsx) is, as there's no address bar there.
const StaticHref = createContext<string | null>(null);

export function StaticLocation({ href, children }: { href: string; children: ReactNode }) {
  return <StaticHref.Provider value={href}>{children}</StaticHref.Provider>;
}

/** The path and the query, e.g. "/login?next=/pricing". */
export function useLocation(): { path: string; query: URLSearchParams } {
  const staticHref = useContext(StaticHref);
  const href = useSyncExternalStore(subscribe, current, () => staticHref ?? current());
  const [path = "/", search = ""] = href.split("?", 2);
  return { path, query: new URLSearchParams(search) };
}

/** A link that stays on the page for paths on this site. */
export function Link({ href, onClick, ...props }: ComponentProps<"a"> & { href: string }) {
  const click = (e: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(e);
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    if (!href.startsWith("/") || href.startsWith("//")) return;
    e.preventDefault();
    navigate(href);
  };
  return <a href={href} onClick={click} {...props} />;
}

export function useTitle(title: string) {
  useEffect(() => {
    document.title = title;
  }, [title]);
}
