// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Your shells, in any of your workspaces, in one place: a bar on the left
// to switch between them, open another or close one, and the terminal in
// front filling the rest. Each shell is a restty
// (https://github.com/wiedymi/restty), libghostty-vt in WebAssembly drawn
// with WebGPU or WebGL2, as `uc agent serve` has it, kept running while
// another is in front. The API hands out a ticket for each shell and relays
// it to the usecode daemon on the workspace's cluster (lib/api's relay.rs).
// The address names the open shells, so a reload opens them again.

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ArrowLeft, Maximize2, Minimize2, PanelLeft, Pencil, Plus, RotateCcw, SquareTerminal, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dot, type State } from "@/components/console";
import { apiUrl, get, post, useMe } from "@/lib/api";
import { navigate, useLocation, useTitle } from "@/lib/router";
import { takeShells } from "@/lib/shells";
import {
  endedWith,
  noShell,
  SHELL_NAME_MAX,
  shellName,
  shellsHref,
  shellsIn,
  shellSocketUrl,
  ticketPath,
  type ShellRef,
  type Workspace,
  type WorkspaceList,
} from "@/lib/workspaces";
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

const TIP_DISMISSED = "shell.lock-tip.dismissed";

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

/** What the bar says in full screen. */
const fullNote = (locked: boolean) =>
  locked
    ? "Every key goes to the shell; hold Esc to leave"
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
  // F11 goes into our full screen, the one with the keyboard locked, rather
  // than the browser's, which locks nothing. Only into it: in full screen
  // every key is the shell's, F11 too, and holding Esc is the one way out.
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key !== "F11" || e.ctrlKey || e.altKey || e.metaKey || e.shiftKey) return;
      if (document.fullscreenElement) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      void toggle();
    };
    window.addEventListener("keydown", key, { capture: true });
    return () => window.removeEventListener("keydown", key, { capture: true });
  }, [toggle]);
  return { full, locked, toggle };
}

/** Your workspaces, looked up again each time `tick` changes. */
function useWorkspaces(enabled: boolean, tick: number) {
  const [list, setList] = useState<Workspace[] | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!enabled) return;
    const ctl = new AbortController();
    get<WorkspaceList>("/workspaces", ctl.signal)
      .then((l) => {
        setList(l.workspaces);
        setError("");
      })
      .catch((err: Error) => {
        if (!ctl.signal.aborted) setError(err.message);
      });
    return () => ctl.abort();
  }, [enabled, tick]);
  return { list, error };
}

/** One open shell; `id` tells two in the same workspace apart. */
type Tab = ShellRef & { id: number };

/** Where a shell is: opening, open, or over (and why). */
type Life = { phase: "opening" } | { phase: "open" } | { phase: "over"; why: string; clean: boolean };

const DOT: Record<Life["phase"], State> = { opening: "reconciling", open: "settled", over: "failed" };
const dotOf = (life: Life | undefined): State =>
  !life ? "reconciling" : life.phase === "over" && life.clean ? "desired" : DOT[life.phase];

const NAV_HIDDEN = "shell.nav.hidden";

let nextId = 1;

export function Shell() {
  const { me } = useMe();
  const { query } = useLocation();
  const first = useMemo(() => shellsIn(query), []); // eslint-disable-line react-hooks/exhaustive-deps

  const [tabs, setTabs] = useState<Tab[]>(() => first.shells.map((s) => ({ ...s, id: nextId++ })));
  const [active, setActive] = useState<number | null>(() => tabs[first.at]?.id ?? null);
  const [lives, setLives] = useState<Record<number, Life>>({});
  const [focusTick, setFocusTick] = useState(0);
  const [picking, setPicking] = useState(false);
  const [listTick, setListTick] = useState(0);
  const [navHidden, setNavHidden] = useState(() => typeof localStorage !== "undefined" && localStorage.getItem(NAV_HIDDEN) === "1");

  const front = tabs.find((t) => t.id === active) ?? null;
  useTitle(front ? `${front.ws} · usecode.dev` : "Shells · usecode.dev");

  useEffect(() => {
    if (me && !me.user) navigate(`/login?next=${encodeURIComponent(`/shell?${query}`)}`, { replace: true });
  }, [me, query]);

  const { list, error } = useWorkspaces(Boolean(me?.user), listTick);
  const hostName = useCallback((host: string) => list?.find((w) => w.host_id === host)?.host_name ?? "", [list]);

  // The address names the open shells and the one in front.
  useEffect(() => {
    if (!me?.user) return;
    const at = Math.max(0, tabs.findIndex((t) => t.id === active));
    history.replaceState(null, "", shellsHref(tabs, at));
  }, [me, tabs, active]);

  const refocus = useCallback(() => setFocusTick((n) => n + 1), []);
  const { full, locked, toggle } = useFullscreen(refocus);
  const { tip, dismiss } = useLockTip();

  const add = useCallback((host: string, ws: string, typed?: string) => {
    const id = nextId++;
    const name = shellName(typed);
    setTabs((ts) => [...ts, name ? { host, ws, name, id } : { host, ws, id }]);
    setActive(id);
    setPicking(false);
  }, []);

  // The shell being renamed, if one is.
  const [editing, setEditing] = useState<number | null>(null);
  const rename = useCallback(
    (id: number, typed: string | null) => {
      setEditing(null);
      if (typed !== null) {
        const name = shellName(typed);
        setTabs((ts) => ts.map((t) => (t.id === id ? (({ name: _, ...rest }) => (name ? { ...rest, name } : rest))(t) : t)));
      }
      refocus();
    },
    [refocus],
  );

  const close = useCallback((id: number) => {
    setTabs((ts) => ts.filter((t) => t.id !== id));
    setLives(({ [id]: _, ...others }) => others);
    setFocusTick((n) => n + 1);
  }, []);

  // The one in front closed: the one below comes to the front, else the one above.
  const frontAt = useRef(0);
  const at = tabs.findIndex((t) => t.id === active);
  if (at >= 0) frontAt.current = at;
  useEffect(() => {
    if (at < 0 && tabs.length > 0) setActive(tabs[Math.min(frontAt.current, tabs.length - 1)]!.id);
    if (tabs.length === 0 && active !== null) setActive(null);
  }, [at, tabs, active]);

  const show = useCallback((id: number) => {
    setActive(id);
    setFocusTick((n) => n + 1);
  }, []);

  const setLife = useCallback((id: number, life: Life) => setLives((ls) => ({ ...ls, [id]: life })), []);

  // Shells asked for on other pages (the workspaces page) come here.
  useEffect(() => (me?.user ? takeShells(add) : undefined), [me, add]);

  // Out of full screen, Ctrl+W meant for a shell would close the tab: while
  // one is open, the browser asks first.
  const anyOpen = useRef(false);
  anyOpen.current = Object.values(lives).some((l) => l.phase === "open");
  useEffect(() => {
    const guard = (e: BeforeUnloadEvent) => {
      if (!anyOpen.current) return;
      e.preventDefault();
      e.returnValue = "";
    };
    window.addEventListener("beforeunload", guard);
    return () => window.removeEventListener("beforeunload", guard);
  }, []);

  const toggleNav = () => {
    setNavHidden((h) => {
      localStorage.setItem(NAV_HIDDEN, h ? "0" : "1");
      return !h;
    });
    refocus();
  };

  const pick = (open: boolean) => {
    setPicking(open);
    if (open) setListTick((n) => n + 1);
    else refocus();
  };

  if (!me?.user) return null;

  // The name you gave a shell, else its workspace's: two shells in the same
  // workspace, the second is "dev 2".
  const defaults = new Map<number, string>();
  const labels = new Map<number, string>();
  const seen = new Map<string, number>();
  for (const t of tabs) {
    const key = `${t.host}/${t.ws}`;
    const n = (seen.get(key) ?? 0) + 1;
    seen.set(key, n);
    const plain = n > 1 ? `${t.ws} ${n}` : t.ws;
    defaults.set(t.id, plain);
    labels.set(t.id, t.name ?? plain);
  }
  // What a new shell in `ws` is called when you don't name it.
  const nextDefault = (host: string, ws: string) => {
    const n = (seen.get(`${host}/${ws}`) ?? 0) + 1;
    return n > 1 ? `${ws} ${n}` : ws;
  };

  return (
    <div className="flex h-svh flex-col bg-[#282828] text-[#d5c4a1]">
      <div className="flex h-9 shrink-0 items-center gap-3 border-b border-white/10 px-3 text-xs">
        <button
          type="button"
          onClick={toggleNav}
          title={navHidden ? "Show your shells" : "Hide the bar"}
          aria-label={navHidden ? "Show your shells" : "Hide the bar"}
          className="text-[#a89984] hover:text-[#ebdbb2]"
        >
          <PanelLeft className="size-3.5" />
        </button>
        <a href="/workspaces" className="flex items-center gap-1.5 text-[#a89984] hover:text-[#ebdbb2]">
          <ArrowLeft className="size-3.5" />
          Workspaces
        </a>
        {front && editing === front.id && navHidden ? (
          <NameInput value={front.name ?? ""} placeholder={defaults.get(front.id) ?? front.ws} onDone={(n) => rename(front.id, n)} />
        ) : front ? (
          <button
            type="button"
            onDoubleClick={() => setEditing(front.id)}
            title="Double-click to rename this shell"
            className="truncate font-mono text-[#ebdbb2]"
          >
            {labels.get(front.id)}
          </button>
        ) : (
          <span className="truncate font-mono text-[#ebdbb2]">Shells</span>
        )}
        {front && (front.name || hostName(front.host)) ? (
          <span className="truncate text-[#a89984]">
            {front.name ? front.ws : null}
            {front.name && hostName(front.host) ? " " : null}
            {hostName(front.host) ? `on ${hostName(front.host)}` : null}
          </span>
        ) : null}
        {full ? (
          <span className="ml-auto truncate text-[#a89984]" role="status">
            {fullNote(locked)}
          </span>
        ) : null}
        <button
          type="button"
          onClick={() => void toggle()}
          title={
            full
              ? locked
                ? "Leave full screen (or hold Esc)"
                : "Leave full screen (or press Esc)"
              : "Full screen (F11): Esc, Ctrl+W and the browser's other keys go to the shell"
          }
          className={`${full ? "" : "ml-auto "}flex shrink-0 items-center gap-1.5 text-[#a89984] hover:text-[#ebdbb2]`}
        >
          {full ? <Minimize2 className="size-3.5" /> : <Maximize2 className="size-3.5" />}
          {full ? "Exit full screen" : "Full screen"}
          {full ? null : <kbd className="font-mono text-[10px] opacity-60">F11</kbd>}
        </button>
      </div>

      <div className="flex min-h-0 flex-1">
        {navHidden ? null : (
          <nav aria-label="Your shells" className="relative flex w-56 shrink-0 flex-col border-r border-white/10 bg-[#1d2021] text-xs">
            <div className="flex h-8 shrink-0 items-center justify-between px-3 text-[#a89984]">
              <span>Shells</span>
              <button
                type="button"
                onClick={() => pick(!picking)}
                // Not a click away from the picker: this button toggles it.
                onMouseDown={(e) => e.stopPropagation()}
                title="Open a shell in one of your workspaces"
                aria-label="New shell"
                aria-expanded={picking}
                className="hover:text-[#ebdbb2]"
              >
                <Plus className="size-4" />
              </button>
            </div>
            <ul className="min-h-0 flex-1 overflow-y-auto pb-2">
              {tabs.map((t) => (
                <li key={t.id}>
                  <div
                    className={`group mx-1.5 flex items-center gap-2 rounded px-2 py-1.5 ${
                      t.id === active ? "bg-white/10 text-[#ebdbb2]" : "text-[#d5c4a1] hover:bg-white/5"
                    }`}
                  >
                    {editing === t.id ? (
                      <div className="flex min-w-0 flex-1 items-center gap-2">
                        <Dot state={dotOf(lives[t.id])} />
                        <NameInput value={t.name ?? ""} placeholder={defaults.get(t.id) ?? t.ws} onDone={(n) => rename(t.id, n)} />
                      </div>
                    ) : (
                      <button
                        type="button"
                        onClick={() => show(t.id)}
                        onDoubleClick={() => setEditing(t.id)}
                        aria-current={t.id === active ? "page" : undefined}
                        className="flex min-w-0 flex-1 items-center gap-2 text-left"
                        title={lives[t.id]?.phase === "over" ? (lives[t.id] as { why: string }).why : "Double-click to rename"}
                      >
                        <Dot state={dotOf(lives[t.id])} />
                        <span className="min-w-0">
                          <span className="block truncate font-mono">{labels.get(t.id)}</span>
                          {t.name || hostName(t.host) ? (
                            <span className="block truncate text-[11px] text-[#928374]">
                              {[t.name ? t.ws : "", hostName(t.host)].filter(Boolean).join(" on ")}
                            </span>
                          ) : null}
                        </span>
                      </button>
                    )}
                    {editing === t.id ? null : (
                      <button
                        type="button"
                        onClick={() => setEditing(t.id)}
                        title="Rename this shell"
                        aria-label={`Rename ${labels.get(t.id)}`}
                        className="invisible shrink-0 text-[#a89984] group-hover:visible hover:text-[#ebdbb2] focus-visible:visible"
                      >
                        <Pencil className="size-3" />
                      </button>
                    )}
                    <button
                      type="button"
                      onClick={() => close(t.id)}
                      title="Close this shell"
                      aria-label={`Close ${labels.get(t.id)}`}
                      className="invisible shrink-0 text-[#a89984] group-hover:visible hover:text-[#ebdbb2] focus-visible:visible"
                    >
                      <X className="size-3.5" />
                    </button>
                  </div>
                </li>
              ))}
            </ul>
            {picking ? (
              <div className="absolute top-8 right-1.5 left-1.5 z-20 max-h-[70%] overflow-y-auto rounded-md border border-white/10 bg-[#282828] p-1 shadow-lg">
                <Picker
                  list={list}
                  error={error}
                  nameFor={(w) => nextDefault(w.host_id, w.name)}
                  onPick={(w, name) => add(w.host_id, w.name, name)}
                  onClose={() => pick(false)}
                />
              </div>
            ) : null}
          </nav>
        )}

        <main className="relative min-w-0 flex-1 overflow-hidden">
          {tabs.map((t) => (
            <Pane
              key={t.id}
              tab={t}
              label={labels.get(t.id) ?? t.ws}
              active={t.id === active}
              focusTick={focusTick}
              onLife={setLife}
            />
          ))}
          {tabs.length === 0 ? (
            <div className="mx-auto max-w-md p-6 text-sm">
              <h1 className="mb-1 text-base text-[#ebdbb2]">No shells open</h1>
              <p className="mb-4 text-[#a89984]">Pick one of your workspaces for a shell in it.</p>
              <div className="rounded-md border border-white/10 p-1">
                <Picker
                  list={list}
                  error={error}
                  nameFor={(w) => nextDefault(w.host_id, w.name)}
                  onPick={(w, name) => add(w.host_id, w.name, name)}
                />
              </div>
            </div>
          ) : null}
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
        </main>
      </div>
    </div>
  );
}

/** Your workspaces to open a shell in; the ones without a shell to be had say why. */
function Picker({
  list,
  error,
  nameFor,
  onPick,
  onClose,
}: {
  list: Workspace[] | null;
  error: string;
  /** What a shell in `w` is called unless you name it. */
  nameFor: (w: Workspace) => string;
  onPick: (w: Workspace, name: string) => void;
  onClose?: () => void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [name, setName] = useState("");
  const open = list?.filter((w) => !noShell(w)) ?? [];
  useEffect(() => {
    if (!onClose) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) onClose();
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [onClose]);

  return (
    <div ref={box} className="text-xs">
      <label className="block px-2 pt-1.5 pb-2">
        <span className="mb-1 block text-[11px] text-[#928374]">Name the new shell (optional)</span>
        <input
          autoFocus
          value={name}
          maxLength={SHELL_NAME_MAX}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            // With one workspace to pick, Enter opens the shell in it.
            if (e.key === "Enter" && open.length === 1) onPick(open[0]!, name);
          }}
          placeholder={open.length === 1 ? `${nameFor(open[0]!)}, Enter opens it` : "build, logs, server…"}
          className="w-full rounded border border-white/10 bg-[#1d2021] px-2 py-1 font-mono text-[#ebdbb2] outline-none placeholder:text-[#665c54] focus:border-[#d79921]/60"
        />
      </label>
      {error ? <p className="px-2 py-1.5 text-[#fb4934]">{error}</p> : null}
      {list === null && !error ? <p className="px-2 py-1.5 text-[#a89984]">Loading your workspaces…</p> : null}
      {list !== null && list.length === 0 ? (
        <p className="px-2 py-1.5 text-[#a89984]">
          No workspaces yet;{" "}
          <a href="/workspaces" className="underline hover:text-[#ebdbb2]">
            here's how to make one
          </a>
          .
        </p>
      ) : null}
      {list?.map((w) => {
        const why = noShell(w);
        return (
          <button
            key={`${w.cluster_id || w.host_id}/${w.name}`}
            type="button"
            disabled={Boolean(why)}
            onClick={() => onPick(w, name)}
            title={why ?? `A shell in ${w.name}, as ${w.user || "you"}, called ${shellName(name) ?? nameFor(w)}`}
            className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-white/10 disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent"
          >
            <SquareTerminal className="size-3.5 shrink-0 text-[#a89984]" />
            <span className="min-w-0">
              <span className="block truncate font-mono text-[#ebdbb2]">{w.name}</span>
              <span className="block truncate text-[11px] text-[#928374]">{why ?? `on ${w.host_name}`}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}

/**
 * One shell: its terminal, kept running when another is in front (hidden,
 * not taken down, so it keeps its size and what it showed).
 */
function Pane({
  tab,
  label,
  active,
  focusTick,
  onLife,
}: {
  tab: Tab;
  label: string;
  active: boolean;
  focusTick: number;
  onLife: (id: number, life: Life) => void;
}) {
  const { id, host, ws } = tab;
  const root = useRef<HTMLDivElement>(null);
  const term = useRef<Restty | null>(null);
  /** Why the shell is over, the first reason heard; null while it isn't. */
  const ended = useRef<string | null>(null);
  const [status, setStatus] = useState("Loading the terminal…");
  const [over, setOver] = useState<string | null>(null);

  const end = useCallback(
    (why: string, clean = false) => {
      ended.current ??= why;
      setOver(ended.current);
      setStatus("");
      onLife(id, { phase: "over", why: ended.current, clean });
    },
    [id, onLife],
  );

  const open = useCallback(async () => {
    const t = term.current;
    if (!t) return;
    ended.current = null;
    setOver(null);
    setStatus("Connecting…");
    onLife(id, { phase: "opening" });
    try {
      const { ticket } = await post<{ ticket: string }>(ticketPath(host, ws));
      t.clearScreen();
      t.connectPty(shellSocketUrl(apiUrl(), ticket));
    } catch (err) {
      end((err as Error).message);
    }
  }, [id, host, ws, end, onLife]);

  useEffect(() => {
    if (!root.current) return;
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
                  onLife(id, { phase: "open" });
                  cb.onConnect?.();
                },
                onError(message, errors) {
                  end(message || "The shell couldn't open.");
                  cb.onError?.(message, errors);
                },
                onExit(code) {
                  end(endedWith(code), code === 0);
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
            createInitialPane: { focus: false },
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
    // Made once per shell; `open` and `end` only change with it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // In front: the keys go here. Again after full screen comes or goes, the
  // bar shows or hides, or the same shell is picked again.
  useEffect(() => {
    if (!active) return;
    const t = setTimeout(() => term.current?.focus(), 0);
    return () => clearTimeout(t);
  }, [active, focusTick, status]);

  return (
    <div className={`absolute inset-0 ${active ? "" : "pointer-events-none invisible"}`} aria-hidden={!active}>
      <div ref={root} className="absolute inset-0 p-1.5" aria-label={`A shell in ${label}`} />
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
    </div>
  );
}

/**
 * A shell's name, being typed: Enter or a click away keeps it, Esc puts the
 * old one back. Blank goes back to the workspace's name.
 */
function NameInput({
  value,
  placeholder,
  onDone,
}: {
  value: string;
  placeholder: string;
  /** The name typed, or null when Esc took it back. */
  onDone: (name: string | null) => void;
}) {
  const done = useRef(false);
  const finish = (name: string | null) => {
    if (done.current) return;
    done.current = true;
    // Left as the workspace's name: no name of its own.
    onDone(name !== null && name.trim() === placeholder ? "" : name);
  };
  return (
    <input
      autoFocus
      defaultValue={value || placeholder}
      maxLength={SHELL_NAME_MAX}
      placeholder={placeholder}
      aria-label="The shell's name"
      onFocus={(e) => e.currentTarget.select()}
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Enter") finish(e.currentTarget.value);
        else if (e.key === "Escape") finish(null);
      }}
      onBlur={(e) => finish(e.currentTarget.value)}
      className="min-w-0 flex-1 rounded border border-[#d79921]/60 bg-[#1d2021] px-1.5 py-0.5 font-mono text-xs text-[#ebdbb2] outline-none"
    />
  );
}

function Note({ children }: { children: ReactNode }) {
  return (
    <div className="pointer-events-none absolute right-3 bottom-3 z-10 rounded bg-[#1d2021]/90 px-2 py-0.5 font-mono text-xs text-[#a89984]">
      {children}
    </div>
  );
}
