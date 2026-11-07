// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// One shells page for all your shells: a shell asked for anywhere on the
// console (the workspaces page) goes to the shells page already open in
// another tab, if there is one, rather than to a page of its own.
//
// The open shells page that holds the LEADER lock is the one that takes
// them, over a BroadcastChannel; when it closes, the next one takes over.

import { shellHref, type Workspace } from "@/lib/workspaces";

const CHANNEL = "uc-shells";
const LEADER = "uc-shells-leader";
/** The tab's name, so a link can find it again. */
export const SHELLS_WINDOW = "uc-shells";

/** Asks the open shells page to open a shell. */
type Open = { type: "open"; host: string; ws: string; nonce: string };
/** The shells page took it. */
type Opened = { type: "opened"; nonce: string };

/** The tab this page opened the shells page in, to bring it to the front. */
let shellsTab: Window | null = null;

/**
 * Opens a shell in `w`: in the shells page open in another tab when there is
 * one ("handed"), else in a new tab ("opened").
 */
export async function openShell(w: Pick<Workspace, "host_id" | "name">): Promise<"handed" | "opened"> {
  const held = await navigator.locks?.query().then(
    (s) => s.held?.some((l) => l.name === LEADER) ?? false,
    () => false,
  );
  if (held) {
    const nonce = crypto.randomUUID();
    const ch = new BroadcastChannel(CHANNEL);
    const taken = await new Promise<boolean>((done) => {
      const t = setTimeout(() => done(false), 1000);
      ch.onmessage = (e: MessageEvent<Opened>) => {
        if (e.data?.type === "opened" && e.data.nonce === nonce) {
          clearTimeout(t);
          done(true);
        }
      };
      ch.postMessage({ type: "open", host: w.host_id, ws: w.name, nonce } satisfies Open);
    });
    ch.close();
    if (taken) {
      if (shellsTab && !shellsTab.closed) shellsTab.focus();
      return "handed";
    }
  }
  shellsTab = window.open(shellHref(w), SHELLS_WINDOW);
  return "opened";
}

/**
 * Takes the shells asked for elsewhere, while this is the shells page in
 * charge; `open` adds one. Returns the way to stop.
 */
export function takeShells(open: (host: string, ws: string) => void): () => void {
  window.name = SHELLS_WINDOW;
  const ch = new BroadcastChannel(CHANNEL);
  const stop = new AbortController();
  navigator.locks
    ?.request(LEADER, { signal: stop.signal }, () => {
      ch.onmessage = (e: MessageEvent<Open>) => {
        const m = e.data;
        if (m?.type !== "open" || !m.host || !m.ws) return;
        open(m.host, m.ws);
        ch.postMessage({ type: "opened", nonce: m.nonce } satisfies Opened);
      };
      // Held for as long as the page is.
      return new Promise<void>((done) => stop.signal.addEventListener("abort", () => done()));
    })
    .catch(() => {});
  return () => {
    stop.abort();
    ch.close();
  };
}
