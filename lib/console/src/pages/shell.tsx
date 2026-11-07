// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// A shell in one of your workspaces, the whole page a terminal: restty
// (https://github.com/wiedymi/restty), libghostty-vt in WebAssembly drawn
// with WebGPU or WebGL2, as `uc agent serve` has it. The API hands out a
// ticket for the shell and relays it to the usecode daemon on the
// workspace's cluster (lib/api's relay.rs).

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowLeft, Maximize2, Minimize2, RotateCcw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { apiUrl, post, useMe } from "@/lib/api";
import { navigate, useLocation, useTitle } from "@/lib/router";
import { endedWith, shellSocketUrl, ticketPath } from "@/lib/workspaces";
import regular from "@/fonts/CaskaydiaMonoNF-Regular.woff2?url";
import bold from "@/fonts/CaskaydiaMonoNF-Bold.woff2?url";
import semiBold from "@/fonts/CaskaydiaMonoNF-SemiBold.woff2?url";
import icons from "@/fonts/CaskaydiaMonoNF-icons.woff2?url";

type Restty = import("restty").Restty;

// Cascadia Mono Nerd Font, the console's own: the letters, then the icons.
const FONTS = [
  { url: regular, name: "CaskaydiaMono NF Regular", weight: 400 },
  { url: bold, name: "CaskaydiaMono NF Bold", weight: 700 },
  { url: semiBold, name: "CaskaydiaMono NF SemiBold", weight: 600 },
  { url: icons, name: "CaskaydiaMono NF Icons", weight: 400 },
];

// The Keyboard Lock API (Chrome, Edge, Opera), not in TypeScript's DOM types.
type KeyboardLock = { lock(keys?: string[]): Promise<void>; unlock(): void };
const keyboard = () => (navigator as Navigator & { keyboard?: KeyboardLock }).keyboard;

/**
 * Brave's Shields take `navigator.keyboard` away, against fingerprinting
 * (brave/brave-browser#50719), so there is nothing to lock with until
 * fingerprinting protection is off for the site.
 */
const shieldsHideLock = () =>
  !keyboard() && Boolean((navigator as Navigator & { brave?: unknown }).brave);

/** Why full screen won't give the shell Esc and Ctrl+W here, if it won't. */
type LockTrouble = "shields" | "unsupported" | "blocked";

const TIPS: Record<LockTrouble, string> = {
  shields:
    "Brave's Shields keep Esc and Ctrl+W from the shell, even in full screen. To let them through, click the lion in the address bar, turn off fingerprint blocking for this site, and reload.",
  unsupported:
    "This browser keeps Esc and Ctrl+W from the shell, even in full screen (Chrome and Edge let them through). Ctrl+[ is Esc for the shell.",
  blocked:
    "Keyboard lock is blocked for this site, so Esc and Ctrl+W stay with the browser in full screen. Allow it in the site's settings (the icon left of the address) and reload.",
};

/** Looks for what would keep the keyboard lock from working; null if nothing. */
async function lockTrouble(): Promise<LockTrouble | null> {
  if (shieldsHideLock()) return "shields";
  if (!keyboard()) return "unsupported";
  try {
    const { state } = await navigator.permissions.query({ name: "keyboard-lock" as PermissionName });
    if (state === "denied") return "blocked";
  } catch {
    // No such permission here: nothing in the way.
  }
  return null;
}

/** The tip for what's in the way, once a look found it; until dismissed. */
function useLockTip() {
  const [tip, setTip] = useState<LockTrouble | null>(null);
  useEffect(() => {
    void lockTrouble().then((t) => {
      if (t && localStorage.getItem(TIP_DISMISSED) !== t) setTip(t);
    });
  }, []);
  const dismiss = useCallback(() => {
    if (tip) localStorage.setItem(TIP_DISMISSED, tip);
    setTip(null);
  }, [tip]);
  return { tip: tip && TIPS[tip], dismiss };
}

const TIP_DISMISSED = "shell.lock-tip.dismissed";

/** What the bar says in full screen. */
const fullNote = (locked: boolean) =>
  locked
    ? "Esc and Ctrl+W go to the shell; hold Esc or press F11 to leave"
    : shieldsHideLock()
      ? "Brave's Shields keep Esc: turn off fingerprint blocking for this site, then full screen again. Till then Ctrl+[ is Esc"
      : "This browser keeps Esc to leave full screen; Ctrl+[ is Esc for the shell";

/** Locks the keyboard; whether it took. */
const lockKeys = (kb: KeyboardLock | undefined) =>
  kb ? kb.lock().then(() => true, () => false) : Promise.resolve(false);

/**
 * Whether the page is full screen, whether the keyboard is locked, and the
 * way in and out. Browsers keep Ctrl+W, Ctrl+T, Ctrl+N and such to
 * themselves, so a shell never sees them; the one way a page gets them is
 * full screen with the keyboard locked. Then they all go to the shell, Esc
 * too (vim's), and only holding Esc leaves. Where the lock doesn't take
 * (Firefox, Safari), Esc leaves full screen, and Ctrl+[ is Esc for the shell.
 */
function useFullscreen(onChange: () => void) {
  const [full, setFull] = useState(false);
  const [locked, setLocked] = useState(false);
  useEffect(() => {
    const changed = () => {
      const now = Boolean(document.fullscreenElement);
      setFull(now);
      if (!now) {
        keyboard()?.unlock();
        setLocked(false);
      }
      onChange();
    };
    document.addEventListener("fullscreenchange", changed);
    return () => document.removeEventListener("fullscreenchange", changed);
  }, [onChange]);
  const toggle = useCallback(async () => {
    if (document.fullscreenElement) {
      await document.exitFullscreen().catch(() => {});
      return;
    }
    const kb = keyboard();
    // Lock first, in the same click as the full screen request: the lock
    // waits for full screen, and that is what has Chrome hand Esc over
    // rather than leave on it. keyboardLock is the same ask in the
    // browsers that take it as a full screen option.
    const early = lockKeys(kb);
    const options = { navigationUI: "hide", keyboardLock: "browser" } as FullscreenOptions;
    await document.documentElement.requestFullscreen(options).catch(() => {});
    if (!document.fullscreenElement) {
      kb?.unlock();
      return;
    }
    const ok = (await early) || (await lockKeys(kb));
    setLocked(ok && Boolean(document.fullscreenElement));
  }, []);
  // F11 is our full screen, the one with the keyboard locked, rather than
  // the browser's, which locks nothing. Caught before the terminal sees it.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key !== "F11" || e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      void toggle();
    };
    window.addEventListener("keydown", key, { capture: true });
    return () => window.removeEventListener("keydown", key, { capture: true });
  }, [toggle]);
  return { full, locked, toggle };
}

export function Shell() {
  const { me } = useMe();
  const { query } = useLocation();
  const host = query.get("host") ?? "";
  const name = query.get("ws") ?? "";
  useTitle(name ? `${name} · usecode.dev` : "Shell · usecode.dev");

  useEffect(() => {
    if (me && !me.user) navigate(`/login?next=${encodeURIComponent(`/shell?${query}`)}`, { replace: true });
  }, [me, query]);

  const root = useRef<HTMLDivElement>(null);
  const term = useRef<Restty | null>(null);
  /** Why the shell is over, the first reason heard; null while it isn't. */
  const ended = useRef<string | null>(null);
  const [status, setStatus] = useState("Loading the terminal…");
  const [over, setOver] = useState<string | null>(null);
  const refocus = useCallback(() => term.current?.focus(), []);
  const { full, locked, toggle } = useFullscreen(refocus);
  const { tip, dismiss } = useLockTip();

  // Out of full screen, Ctrl+W meant for the shell would close the tab: while
  // the shell is open, the browser asks first.
  useEffect(() => {
    const guard = (e: BeforeUnloadEvent) => {
      if (ended.current !== null || !term.current?.isPtyConnected()) return;
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", guard);
    return () => window.removeEventListener("beforeunload", guard);
  }, []);

  const end = useCallback((why: string) => {
    ended.current ??= why;
    setOver(ended.current);
    setStatus("");
  }, []);

  const open = useCallback(async () => {
    const t = term.current;
    if (!t) return;
    ended.current = null;
    setOver(null);
    setStatus("Connecting…");
    try {
      const { ticket } = await post<{ ticket: string }>(ticketPath(host, name));
      t.clearScreen();
      t.connectPty(shellSocketUrl(apiUrl(), ticket));
      t.focus();
    } catch (err) {
      end((err as Error).message);
    }
  }, [host, name, end]);

  useEffect(() => {
    if (!me?.user || !host || !name || !root.current) return;
    let gone = false;
    let made: Restty | null = null;
    import("restty")
      .then(({ Restty, getBuiltinTheme, createWebSocketPtyTransport }) => {
        if (gone || !root.current) return;
        // restty's own transport, told apart so the page knows why it ended.
        const base = createWebSocketPtyTransport();
        const transport: typeof base = {
          ...base,
          connect(options) {
            const cb = options.callbacks;
            return base.connect({
              ...options,
              callbacks: {
                ...cb,
                onConnect() {
                  setStatus("");
                  cb.onConnect?.();
                },
                onError(message, errors) {
                  end(message || "The shell couldn't open.");
                  cb.onError?.(message, errors);
                },
                onExit(code) {
                  end(endedWith(code));
                  cb.onExit?.(code);
                },
                onDisconnect() {
                  end("The connection dropped.");
                  cb.onDisconnect?.();
                },
              },
            });
          },
        };
        made = new Restty({
          root: root.current,
          surface: {
            createInitialPane: { focus: true },
            // One shell, one pane: no splits.
            shortcuts: false,
            searchUi: true,
            defaultContextMenu: true,
          },
          terminal: {
            renderer: "auto",
            fontSize: 15,
            fonts: FONTS,
            ligatures: false,
            theme: getBuiltinTheme("Gruvbox Dark") ?? undefined,
            maxScrollbackBytes: 10_000_000,
          },
          services: { ptyTransport: transport },
        });
        term.current = made;
        void open();
      })
      .catch((err: Error) => end(`The terminal didn't load: ${err.message}`));
    return () => {
      gone = true;
      made?.destroy();
      term.current = null;
    };
  }, [me, host, name, open, end]);

  if (!me?.user) return null;
  if (!host || !name) {
    return (
      <Frame title="Shell">
        <p className="p-4 text-sm">
          No workspace named here; pick one on{" "}
          <a href="/workspaces" className="underline hover:text-[#ebdbb2]">
            your workspaces
          </a>
          .
        </p>
      </Frame>
    );
  }

  return (
    <Frame
      title={name}
      actions={
        <>
          {full ? (
            <span className="ml-auto text-[#a89984]" role="status">
              {fullNote(locked)}
            </span>
          ) : null}
          <button
            type="button"
            onClick={() => void toggle()}
            title={
              full
                ? locked
                  ? "Leave full screen (F11, or hold Esc)"
                  : "Leave full screen (F11 or Esc)"
                : "Full screen (F11): Esc, Ctrl+W and the browser's other keys go to the shell"
            }
            className={`${full ? "" : "ml-auto "}flex items-center gap-1.5 text-[#a89984] hover:text-[#ebdbb2]`}
          >
            {full ? <Minimize2 className="size-3.5" /> : <Maximize2 className="size-3.5" />}
            {full ? "Exit full screen" : "Full screen"}
            <kbd className="font-mono text-[10px] opacity-60">F11</kbd>
          </button>
        </>
      }
    >
      <div ref={root} className="absolute inset-0 p-1.5" aria-label={`A shell in ${name}`} />
      {tip ? (
        <div className="absolute top-3 right-3 z-10 flex max-w-[min(90%,60ch)] items-start gap-3 rounded-md border border-[#d79921]/40 bg-[#1d2021]/95 px-3 py-2 text-sm text-[#d5c4a1] shadow">
          <span role="note">
            <span className="text-[#fabd2f]">Tip: </span>
            {tip}
          </span>
          <button
            type="button"
            onClick={() => {
              dismiss();
              refocus();
            }}
            aria-label="Dismiss the tip"
            className="shrink-0 text-[#a89984] hover:text-[#ebdbb2]"
          >
            <X className="size-4" />
          </button>
        </div>
      ) : null}
      {status ? <Note>{status}</Note> : null}
      {over ? (
        <div className="absolute right-3 bottom-3 z-10 flex max-w-[min(90%,48ch)] items-center gap-3 rounded-md border border-white/10 bg-[#1d2021]/95 px-3 py-2 text-sm text-[#d5c4a1] shadow">
          <span role="status">{over}</span>
          <Button size="sm" variant="secondary" onClick={() => void open()} className="shrink-0 gap-2">
            <RotateCcw />
            Open again
          </Button>
        </div>
      ) : null}
    </Frame>
  );
}

/** The page: a thin bar with the way back, the terminal under it. */
function Frame({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex h-svh flex-col bg-[#282828] text-[#d5c4a1]">
      <div className="flex h-9 shrink-0 items-center gap-3 border-b border-white/10 px-3 text-xs">
        <a href="/workspaces" className="flex items-center gap-1.5 text-[#a89984] hover:text-[#ebdbb2]">
          <ArrowLeft className="size-3.5" />
          Workspaces
        </a>
        <span className="font-mono text-[#ebdbb2]">{title}</span>
        {actions}
      </div>
      <div className="relative min-h-0 flex-1 overflow-hidden">{children}</div>
    </div>
  );
}

function Note({ children }: { children: ReactNode }) {
  return (
    <div className="pointer-events-none absolute right-3 bottom-3 z-10 rounded bg-[#1d2021]/90 px-2 py-0.5 font-mono text-xs text-[#a89984]">
      {children}
    </div>
  );
}
