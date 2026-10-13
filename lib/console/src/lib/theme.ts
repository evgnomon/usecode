// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { useEffect, useSyncExternalStore } from "react";

/** "auto" follows the system; the others are what the visitor picked. */
export type Theme = "auto" | "light" | "dark";
export const THEMES: readonly Theme[] = ["auto", "light", "dark"];
export const nextTheme = (t: Theme): Theme => THEMES[(THEMES.indexOf(t) + 1) % THEMES.length]!;

const KEY = "theme";
const system = () => matchMedia("(prefers-color-scheme: dark)");

// What was picked in this tab, for when the browser won't keep it.
let picked: Theme | null = null;
const listeners = new Set<() => void>();

function stored(): Theme {
  if (picked) return picked;
  try {
    return THEMES.find((t) => t === localStorage.getItem(KEY)) ?? "auto";
  } catch {
    return "auto";
  }
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function apply(theme: Theme) {
  const dark = theme === "dark" || (theme === "auto" && system().matches);
  document.documentElement.classList.toggle("dark", dark);
}

function setTheme(theme: Theme) {
  picked = theme;
  try {
    if (theme === "auto") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, theme);
  } catch {
    // Private windows may not keep it; the theme still changes.
  }
  apply(theme);
  listeners.forEach((l) => l());
}

/**
 * The visitor's theme, kept in this browser, and a way to change it. A
 * prerendered page says "auto" until it hydrates, then the stored one.
 */
export function useTheme(): [Theme, (t: Theme) => void] {
  const theme = useSyncExternalStore(subscribe, stored, () => "auto" as Theme);
  // public/theme.js set the page up before it painted; here "auto" keeps
  // following the system.
  useEffect(() => {
    if (theme !== "auto") return;
    const mq = system();
    const follow = () => apply("auto");
    mq.addEventListener("change", follow);
    return () => mq.removeEventListener("change", follow);
  }, [theme]);
  return [theme, setTheme];
}
