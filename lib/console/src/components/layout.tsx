// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import type { ReactNode } from "react";
import { Monitor, Moon, Sun } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Kbd, KeyLegend } from "@/components/kbd";
import { REPO_URL } from "@/content";
import { post, useMe } from "@/lib/api";
import { HOTKEYS, useHotkeys } from "@/lib/keys";
import { Link, navigate, useLocation } from "@/lib/router";
import { nextTheme, useTheme, type Theme } from "@/lib/theme";
import { cn } from "@/lib/utils";

const THEME_LABEL: Record<Theme, string> = { auto: "Auto", light: "Light", dark: "Dark" };
const THEME_ICON = { auto: Monitor, light: Sun, dark: Moon };

/** Cycles auto, light, dark, remembered in this browser. */
export function ThemeSwitch() {
  const [theme, setTheme] = useTheme();
  const next = nextTheme(theme);
  const Icon = THEME_ICON[theme];
  return (
    <Button
      variant="outline"
      size="xs"
      className="h-7 bg-transparent text-muted-foreground shadow-none hover:bg-transparent hover:text-foreground"
      aria-label={`Theme: ${THEME_LABEL[theme]}. Switch to ${THEME_LABEL[next]}`}
      aria-keyshortcuts="t"
      onClick={() => setTheme(next)}
    >
      <Icon strokeWidth={1.5} />
      {THEME_LABEL[theme]}
      <KeyLegend k="t" />
    </Button>
  );
}

function NavLink({ href, children }: { href: string; children: ReactNode }) {
  const { path } = useLocation();
  return (
    <Link
      href={href}
      aria-current={path === href ? "page" : undefined}
      className={cn("text-[13px] text-muted-foreground transition-colors hover:text-foreground", path === href && "text-foreground")}
    >
      {children}
    </Link>
  );
}

function Header() {
  const { me, refresh } = useMe();
  const logout = async () => {
    await post("/api/logout");
    await refresh();
    navigate("/");
  };
  return (
    <header className="sticky top-0 z-10 border-b bg-background">
      <div className="wrap flex min-h-13 flex-wrap items-center justify-between gap-4 py-2 sm:py-0">
        <Link href="/" className="font-medium">
          usecode<span className="font-normal text-muted-foreground">.dev</span>
        </Link>
        <nav className="flex flex-wrap items-center gap-4">
          <NavLink href="/">Home</NavLink>
          <NavLink href="/pricing">Pricing</NavLink>
          {me?.user ? (
            <>
              <NavLink href="/account">Account</NavLink>
              <button type="button" onClick={logout} className="cursor-pointer text-[13px] text-muted-foreground transition-colors hover:text-foreground">
                Log out
              </button>
            </>
          ) : me ? (
            <>
              <NavLink href="/login">Log in</NavLink>
              <Button asChild size="sm" className="h-7 text-xs" aria-keyshortcuts="s">
                <Link href="/signup">
                  Sign up
                  <KeyLegend k="s" />
                </Link>
              </Button>
            </>
          ) : null}
          <ThemeSwitch />
        </nav>
      </div>
    </header>
  );
}

function Footer() {
  const { me } = useMe();
  const keys = HOTKEYS.filter((k) => {
    if (k.href === "/account") return Boolean(me?.user);
    if (k.href === "/login" || k.href === "/signup") return me != null && !me.user;
    return true;
  });
  return (
    <footer className="border-t py-5 text-xs">
      <div className="wrap flex flex-wrap justify-between gap-4">
        <span className="text-muted-foreground">usecode, container for agents</span>
        <a href={REPO_URL} className="text-primary hover:underline">
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
      <Header />
      <main className="flex-1">{children}</main>
      <Footer />
    </div>
  );
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
