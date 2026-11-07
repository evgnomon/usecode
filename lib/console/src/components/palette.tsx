// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The command palette, as an editor has: Ctrl+Shift+P (Cmd+Shift+P on a Mac)
// anywhere, or the search box in the bar, opens it; type a few letters of a
// page, then Enter. The arrows move, Esc closes. It takes the key ahead of
// the shells too, except in full screen, where every key is the shell's.

import { useCallback, useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { ArrowRight, ExternalLink, LogOut, Monitor, Moon, Search, Sun } from "lucide-react";
import { Kbd } from "@/components/kbd";
import { REPO_URL } from "@/content";
import { useMe } from "@/lib/api";
import { forWho, HOTKEYS } from "@/lib/keys";
import { filter, opensPalette } from "@/lib/palette";
import { navigate, useLocation } from "@/lib/router";
import { useTheme, type Theme } from "@/lib/theme";
import { cn } from "@/lib/utils";

type Command = {
  id: string;
  label: string;
  keywords?: string;
  /** What kind of thing it is, shown faint on the right. */
  kind: string;
  icon: typeof Search;
  /** The page's own key, shown where the page takes it. */
  key?: string;
  current?: boolean;
  run: () => void;
};

const THEMES: { theme: Theme; label: string; icon: typeof Search }[] = [
  { theme: "auto", label: "Auto", icon: Monitor },
  { theme: "light", label: "Light", icon: Sun },
  { theme: "dark", label: "Dark", icon: Moon },
];

/** How the shortcut reads here: Cmd on a Mac, Ctrl elsewhere. */
function useShortcutLabel() {
  const [mac, setMac] = useState(false);
  useEffect(() => setMac(/Mac|iPhone|iPad/.test(navigator.platform)), []);
  return mac ? "⌘⇧P" : "Ctrl+Shift+P";
}

function useCommands(onLogout: () => void): Command[] {
  const { me } = useMe();
  const { path } = useLocation();
  const [theme, setTheme] = useTheme();
  const member = me ? Boolean(me.user) : null;
  const pages: Command[] = HOTKEYS.filter((k) => k.href && forWho(k, member)).map((k) => ({
    id: `page:${k.href}`,
    label: k.label,
    keywords: k.keywords,
    kind: "Page",
    icon: ArrowRight,
    key: k.key,
    current: k.href === path,
    run: () => {
      if (k.href !== path) navigate(k.href!);
    },
  }));
  const themes: Command[] = THEMES.map((t) => ({
    id: `theme:${t.theme}`,
    label: `Theme: ${t.label}`,
    keywords: "colour color appearance mode dark light",
    kind: "Theme",
    icon: t.icon,
    current: t.theme === theme,
    run: () => setTheme(t.theme),
  }));
  const rest: Command[] = [
    {
      id: "source",
      label: "Source on GitHub",
      keywords: "code repository git",
      kind: "Link",
      icon: ExternalLink,
      run: () => window.open(REPO_URL, "_blank", "noopener"),
    },
  ];
  if (me?.user) rest.push({ id: "logout", label: "Log out", keywords: "sign out exit", kind: "Account", icon: LogOut, run: onLogout });
  return [...pages, ...themes, ...rest];
}

/**
 * The search box in the bar, and the palette it opens. `hotkeys` says
 * whether the page takes the letter keys, so the palette shows them;
 * `onClose` gives the focus back (to a shell) when it closes.
 */
export function CommandPalette({
  hotkeys,
  onClose,
  onLogout,
}: {
  hotkeys: boolean;
  onClose?: () => void;
  onLogout: () => void;
}) {
  const [open, setOpen] = useState(false);
  const before = useRef<HTMLElement | null>(null);
  const shortcut = useShortcutLabel();

  const show = useCallback(() => {
    before.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setOpen(true);
  }, []);
  const hide = useCallback(() => {
    setOpen(false);
    if (onClose) onClose();
    else before.current?.focus();
  }, [onClose]);

  // Ahead of everything else (the shells too), so it is the palette's.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (!opensPalette(e) || e.repeat || document.fullscreenElement) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      if (open) hide();
      else show();
    };
    window.addEventListener("keydown", key, { capture: true });
    return () => window.removeEventListener("keydown", key, { capture: true });
  }, [open, show, hide]);

  return (
    <>
      <button
        type="button"
        onClick={() => (open ? hide() : show())}
        title={`Go to a page, or run a command (${shortcut})`}
        aria-label="Go to a page, or run a command"
        aria-keyshortcuts="Control+Shift+P Meta+Shift+P"
        className="flex h-6 shrink-0 items-center gap-2 border border-bar-line px-2 text-bar-muted hover:text-bar-fg"
      >
        <Search className="size-3.5" />
        <span className="max-sm:hidden">Go to…</span>
        <kbd className="font-sans text-[10px] opacity-70 max-sm:hidden">{shortcut}</kbd>
      </button>
      {open ? <Palette hotkeys={hotkeys} onLogout={onLogout} onClose={hide} onRan={() => setOpen(false)} /> : null}
    </>
  );
}

function Palette({
  hotkeys,
  onLogout,
  onClose,
  onRan,
}: {
  hotkeys: boolean;
  onLogout: () => void;
  /** Closed without running anything: the focus goes back. */
  onClose: () => void;
  /** Closed by running a command: it decides where the focus goes. */
  onRan: () => void;
}) {
  const commands = useCommands(onLogout);
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const list = useRef<HTMLUListElement>(null);
  const id = useId();

  const shown = filter(commands, query);
  const at = Math.min(sel, Math.max(0, shown.length - 1));

  useEffect(() => {
    list.current?.querySelector<HTMLElement>(`[data-at="${at}"]`)?.scrollIntoView({ block: "nearest" });
  }, [at]);

  const run = (c: Command | undefined) => {
    if (!c) return;
    // A page or the theme moves on from here; anything else, back where you were.
    if (c.id.startsWith("page:") && !c.current) onRan();
    else onClose();
    c.run();
  };

  const onKey = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    const n = shown.length;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === "ArrowDown" || (e.key === "Tab" && !e.shiftKey) || (e.ctrlKey && e.key === "n")) {
      e.preventDefault();
      if (n) setSel((at + 1) % n);
    } else if (e.key === "ArrowUp" || (e.key === "Tab" && e.shiftKey) || (e.ctrlKey && e.key === "p")) {
      e.preventDefault();
      if (n) setSel((at - 1 + n) % n);
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(shown[at]);
    }
  };

  return (
    <div className="fixed inset-0 z-50" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div
        role="dialog"
        aria-label="Command palette"
        className="absolute top-11 left-1/2 w-[min(36rem,calc(100vw-1.5rem))] -translate-x-1/2 border border-bar-line bg-bar-menu text-xs text-bar-muted shadow-2xl"
      >
        <div className="flex items-center gap-2 border-b border-bar-line px-3">
          <Search className="size-3.5 shrink-0" />
          <input
            autoFocus
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSel(0);
            }}
            onKeyDown={onKey}
            placeholder="Go to a page, or run a command"
            role="combobox"
            aria-expanded="true"
            aria-controls={`${id}-list`}
            aria-activedescendant={shown.length ? `${id}-${at}` : undefined}
            spellCheck={false}
            autoComplete="off"
            className="h-9 w-full bg-transparent text-sm text-bar-fg outline-none placeholder:text-bar-muted"
          />
        </div>
        <ul ref={list} id={`${id}-list`} role="listbox" className="max-h-[min(22rem,60vh)] overflow-y-auto py-1">
          {shown.length === 0 ? <li className="px-3 py-1.5">Nothing matches “{query}”.</li> : null}
          {shown.map((c, n) => {
            const Icon = c.icon;
            return (
              <li
                key={c.id}
                id={`${id}-${n}`}
                data-at={n}
                role="option"
                aria-selected={n === at}
                onMouseMove={() => n !== at && setSel(n)}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => run(c)}
                className={cn("flex cursor-pointer items-center gap-2 px-3 py-1.5", n === at && "bg-bar-hover text-bar-fg")}
              >
                <Icon className="size-3.5 shrink-0" />
                <span className={cn("truncate", n === at ? "text-bar-fg" : "text-bar-fg/90")}>{c.label}</span>
                {c.current ? <span className="shrink-0 opacity-70">· {c.kind === "Page" ? "you are here" : "on"}</span> : null}
                <span className="ml-auto flex shrink-0 items-center gap-2">
                  <span className="opacity-60">{c.kind}</span>
                  {hotkeys && c.key ? <Kbd>{c.key}</Kbd> : null}
                </span>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
