// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The console's shortcut keys: one letter per page, pressed anywhere without
// modifiers (except while typing). Every button that carries one shows it on
// its bottom face, and the footer at the bottom of the page lists them all.

import { useEffect } from "react";
import { navigate, useLocation } from "@/lib/router";
import { useMe } from "@/lib/api";
import { nextTheme, useTheme } from "@/lib/theme";

/** One shortcut: the key that presses it, what it reads as, where it lands. */
export type Hotkey = {
  /** The single character pressed, e.g. "h". */
  key: string;
  /** What it's called in the footer, e.g. "Home". */
  label: string;
  /** The page it goes to, if it's a page. */
  href?: string;
};

/** The console's keys, in the order the footer lists them. */
export const HOTKEYS: Hotkey[] = [
  { key: "h", label: "Home", href: "/" },
  { key: "p", label: "Pricing", href: "/pricing" },
  { key: "a", label: "Account", href: "/account" },
  { key: "l", label: "Log in", href: "/login" },
  { key: "s", label: "Sign up", href: "/signup" },
  { key: "t", label: "Theme" },
  { key: "d", label: "Design", href: "/design" },
];

/** True when a keypress would be typed rather than pressed, so no shortcut takes it. */
const typing = (el: EventTarget | null) =>
  el instanceof HTMLElement && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);

/**
 * Listens for the shortcut keys while the page is up: a letter on its own goes
 * to its page, and "t" cycles the theme. Nothing happens while typing, with a
 * modifier held, or before the page knows who's looking.
 */
export function useHotkeys() {
  const { path } = useLocation();
  const { me } = useMe();
  const [theme, setTheme] = useTheme();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.repeat || typing(e.target)) return;
      const hot = HOTKEYS.find((k) => k.key === e.key);
      if (!hot) return;
      // The keys of pages that aren't for you right now don't go anywhere.
      if (hot.href === "/account" && !me?.user) return;
      if ((hot.href === "/login" || hot.href === "/signup") && me?.user) return;
      e.preventDefault();
      if (!hot.href) {
        setTheme(nextTheme(theme));
        return;
      }
      if (hot.href !== path) navigate(hot.href);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [path, me, theme, setTheme]);
}