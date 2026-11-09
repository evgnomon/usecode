// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { LogOut, Menu, Monitor, Moon, Sun } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Kbd, KeyLegend } from "@/components/kbd";
import { CommandPalette } from "@/components/palette";
import { REPO_URL } from "@/content";
import { post, useMe } from "@/lib/api";
import { forWho, HOTKEYS, useHotkeys } from "@/lib/keys";
import { Link, navigate, useLocation } from "@/lib/router";
import { nextTheme, useTheme, type Theme } from "@/lib/theme";
import { cn } from "@/lib/utils";

const THEME_LABEL: Record<Theme, string> = { auto: "Auto", light: "Light", dark: "Dark" };
const THEME_ICON = { auto: Monitor, light: Sun, dark: Moon };

/** Cycles auto, light, dark, remembered in this browser. */
export function ThemeSwitch({ className }: { className?: string }) {
  const [theme, setTheme] = useTheme();
  const next = nextTheme(theme);
  const Icon = THEME_ICON[theme];
  return (
    <Button
      variant="outline"
      size="xs"
      className={cn(
        "h-6 border-bar-line bg-transparent text-bar-muted shadow-none hover:bg-transparent hover:text-bar-fg",
        className,
      )}
      aria-label={`Theme: ${THEME_LABEL[theme]}. Switch to ${THEME_LABEL[next]}`}
      aria-keyshortcuts="t"
      onClick={() => setTheme(next)}
    >
      <Icon strokeWidth={1.5} />
      <span className="max-sm:hidden">{THEME_LABEL[theme]}</span>
      <KeyLegend k="t" />
    </Button>
  );
}

const ITEM = "flex w-full cursor-pointer items-center justify-between gap-6 px-3 py-1.5 text-left hover:bg-bar-hover hover:text-bar-fg";

/**
 * The site's pages behind the burger at the left of the bar. A <details>, so
 * it opens on the prerendered pages before any JavaScript has run.
 */
function SiteMenu({ hotkeys, onClose, onLogout }: { hotkeys: boolean; onClose?: () => void; onLogout: () => Promise<void> }) {
  const { me } = useMe();
  const { path } = useLocation();
  const box = useRef<HTMLDetailsElement>(null);
  const [open, setOpen] = useState(false);
  const shut = useCallback(() => {
    if (box.current) box.current.open = false;
  }, []);

  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) shut();
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      shut();
    };
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open, shut]);

  const logout = async () => {
    shut();
    await onLogout();
  };

  const member = me ? Boolean(me.user) : null;
  const pages = HOTKEYS.filter((k) => k.href && forWho(k, member));

  return (
    <details
      ref={box}
      className="relative flex items-center self-stretch"
      onToggle={(e) => {
        const now = e.currentTarget.open;
        setOpen(now);
        if (!now) onClose?.();
      }}
    >
      <summary
        title="The site's pages"
        aria-label="Menu"
        className="flex cursor-pointer list-none items-center text-bar-muted hover:text-bar-fg [&::-webkit-details-marker]:hidden"
      >
        <Menu className="size-4" />
      </summary>
      <nav
        aria-label="Site"
        className="absolute top-[calc(100%+1px)] -left-3 w-56 border border-t-0 border-bar-line bg-bar-menu py-1 text-xs text-bar-muted shadow-lg"
      >
        <Brand onClick={shut} className="px-3 pt-1.5 pb-2" />
        {pages.map((k) => (
          <Link
            key={k.href}
            href={k.href!}
            onClick={shut}
            aria-current={path === k.href ? "page" : undefined}
            className={cn(ITEM, path === k.href && "bg-bar-hover text-bar-fg")}
          >
            {k.label}
            {hotkeys ? <Kbd>{k.key}</Kbd> : null}
          </Link>
        ))}
        {me?.user ? (
          <>
            <div className="my-1 border-t border-bar-line" />
            <button type="button" onClick={() => void logout()} className={ITEM}>
              Log out
              <LogOut className="size-3.5" />
            </button>
          </>
        ) : null}
      </nav>
    </details>
  );
}

/** The site's name, for pages that have nothing of their own to show in the bar. */
function Brand({ onClick, className }: { onClick?: () => void; className?: string }) {
  return (
    <Link href="/" onClick={onClick} className={cn("flex items-center gap-2 font-medium text-bar-fg", className)}>
      <img src="/icon.svg" alt="" width={18} height={18} className="size-4.5 rounded-[4px]" />
      <span>
        usecode<span className="font-normal text-bar-muted">.dev</span>
      </span>
    </Link>
  );
}

/**
 * The bar along the top of every page, as an editor has: the site's menu
 * behind a burger at the left, then the page's own (the site's name if it
 * has none), and on the right what the page puts there, before logging in
 * and the theme. The shells bring their own, in their colours (`tone`).
 */
export function TopBar({
  tone,
  hotkeys = false,
  onMenuClose,
  after,
  className,
  children,
}: {
  tone?: "shell";
  /** Whether the page takes the shortcut keys, so the menu shows them. */
  hotkeys?: boolean;
  /** The menu or the command palette closed: give the focus back to what had it. */
  onMenuClose?: () => void;
  after?: ReactNode;
  className?: string;
  children?: ReactNode;
}) {
  const { me, refresh } = useMe();
  const logout = useCallback(async () => {
    await post("/auth/logout");
    await refresh();
    navigate("/");
  }, [refresh]);
  return (
    <header
      className={cn(
        tone === "shell" ? "bar-shell" : "bar",
        "relative z-30 flex h-9 shrink-0 items-center gap-3 border-b border-bar-line bg-bar px-3 text-xs text-bar-muted",
        className,
      )}
    >
      <SiteMenu hotkeys={hotkeys} onClose={onMenuClose} onLogout={logout} />
      <div className="flex min-w-0 items-center gap-3">{children ?? <Brand />}</div>
      <div className="ml-auto flex min-w-0 items-center gap-3">
        <CommandPalette hotkeys={hotkeys} onClose={onMenuClose} onLogout={() => void logout()} />
        {after}
        {me && !me.user ? (
          <>
            <Link href="/login" className="hover:text-bar-fg max-sm:hidden">
              Log in
            </Link>
            <Button asChild size="xs" className="h-6 text-xs" aria-keyshortcuts="s">
              <Link href="/signup">
                Sign up
                <KeyLegend k="s" />
              </Link>
            </Button>
          </>
        ) : null}
        {tone === "shell" ? null : <ThemeSwitch />}
      </div>
    </header>
  );
}

function Footer() {
  const { me } = useMe();
  const keys = HOTKEYS.filter((k) => forWho(k, me ? Boolean(me.user) : null));
  return (
    <footer className="border-t py-5 text-xs">
      <div className="wrap flex flex-wrap justify-between gap-4">
        <span className="text-muted-foreground">usecode, intelligence without overhead</span>
        <a href={REPO_URL} className="-my-1 inline-block py-1 text-primary hover:underline">
          Source on GitHub
        </a>
      </div>
      {/* The keys, listed on the bottom of the page where they'll be looked for. */}
      <div className="wrap mt-3 flex flex-wrap gap-x-4 gap-y-1">
        {keys.map((k) => (
          <span key={k.key} className="text-muted-foreground">
            <Kbd>{k.key}</Kbd> {k.label}
          </span>
        ))}
      </div>
    </footer>
  );
}

export function Layout({ children }: { children: ReactNode }) {
  useHotkeys();
  return (
    <div className="flex min-h-svh flex-col">
      <TopBar hotkeys className="sticky top-0" />
      <main className="flex-1">{children}</main>
      <Footer />
    </div>
  );
}

/**
 * A page filling the window, as the shells do: it brings its own TopBar,
 * and has no footer and no shortcut keys.
 */
export function FillLayout({ children }: { children: ReactNode }) {
  return <div className="flex h-svh flex-col">{children}</div>;
}

/** A page's title and lede, under a rule. */
export function PageHead({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="border-b pt-12 pb-6">
      <div className="wrap">
        <h1 className="mb-3 text-xl">{title}</h1>
        <p className="max-w-[60ch] text-base text-muted-foreground">{children}</p>
      </div>
    </section>
  );
}

export function Section({ className, children }: { className?: string; children: ReactNode }) {
  return <section className={cn("py-8 sm:py-12", className)}>{children}</section>;
}

/** Amber: something wants your attention, nothing is broken. */
export function Notice({ children }: { children: ReactNode }) {
  return (
    <Alert role="status" className="mb-3 border-drift/30 bg-drift-soft px-3 py-2">
      <AlertDescription className="text-foreground">{children}</AlertDescription>
    </Alert>
  );
}

export function ErrorNote({ children }: { children: ReactNode }) {
  return (
    <Alert role="alert" className="mb-3 border-failed/30 bg-failed-soft px-3 py-2">
      <AlertDescription className="text-failed">{children}</AlertDescription>
    </Alert>
  );
}
