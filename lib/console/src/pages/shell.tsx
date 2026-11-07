// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// A shell in one of your workspaces, the whole page a terminal: restty
// (https://github.com/wiedymi/restty), libghostty-vt in WebAssembly drawn
// with WebGPU or WebGL2, as `uc agent serve` has it. The API hands out a
// ticket for the shell and relays it to the usecode daemon on the
// workspace's cluster (lib/api's relay.rs).

import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { ArrowLeft, RotateCcw } from "lucide-react";
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
    <Frame title={name}>
      <div ref={root} className="absolute inset-0 p-1.5" aria-label={`A shell in ${name}`} />
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
function Frame({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex h-svh flex-col bg-[#282828] text-[#d5c4a1]">
      <div className="flex h-9 shrink-0 items-center gap-3 border-b border-white/10 px-3 text-xs">
        <a href="/workspaces" className="flex items-center gap-1.5 text-[#a89984] hover:text-[#ebdbb2]">
          <ArrowLeft className="size-3.5" />
          Workspaces
        </a>
        <span className="font-mono text-[#ebdbb2]">{title}</span>
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
