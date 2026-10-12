// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { useEffect, useState } from "react";

/** "auto" follows the system; the others are what the visitor picked. */
export type Theme = "auto" | "light" | "dark";
export const THEMES: readonly Theme[] = ["auto", "light", "dark"];
export const nextTheme = (t: Theme): Theme => THEMES[(THEMES.indexOf(t) + 1) % THEMES.length]!;

const KEY = "theme";
const system = () => matchMedia("(prefers-color-scheme: dark)");

function stored(): Theme {
  try {
    return THEMES.find((t) => t === localStorage.getItem(KEY)) ?? "auto";
  } catch {
    return "auto";
  }
}

function apply(theme: Theme) {
  const dark = theme === "dark" || (theme === "auto" && system().matches);
  document.documentElement.classList.toggle("dark", dark);
}

/** The visitor's theme, kept in this browser, and a way to change it. */
export function useTheme(): [Theme, (t: Theme) => void] {
  const [theme, setTheme] = useState(stored);
  useEffect(() => {
    apply(theme);
    try {
      if (theme === "auto") localStorage.removeItem(KEY);
      else localStorage.setItem(KEY, theme);
    } catch {
      // Private windows may not keep it; the theme still changes.
    }
    if (theme !== "auto") return;
    const mq = system();
    const follow = () => apply("auto");
    mq.addEventListener("change", follow);
    return () => mq.removeEventListener("change", follow);
  }, [theme]);
  return [theme, setTheme];
}
