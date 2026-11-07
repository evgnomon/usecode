// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Your workspaces: the dev containers (`uc ws`) on your clusters, as their
// controllers report them, each with a shell a click away.

import { useEffect, useState } from "react";
import { Boxes, SquareTerminal } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dot, List, Pill, type State } from "@/components/console";
import { ErrorNote, Section } from "@/components/layout";
import { get, useMe } from "@/lib/api";
import { navigate } from "@/lib/router";
import { openShell, SHELLS_WINDOW } from "@/lib/shells";
import { noShell, shellHref, type Workspace, type WorkspaceList, type WorkspaceState } from "@/lib/workspaces";

/** How often the list looks again while the page is in view. */
const REFRESH_MS = 30_000;

const STATE: Record<WorkspaceState, { state: State; label: string }> = {
  running: { state: "settled", label: "Running" },
  starting: { state: "reconciling", label: "Starting" },
  stopped: { state: "desired", label: "Stopped" },
  unknown: { state: "drift", label: "Unknown" },
};

export function Workspaces() {
  const { me } = useMe();
  useEffect(() => {
    if (me && !me.user) navigate("/login?next=/workspaces", { replace: true });
  }, [me]);

  const [list, setList] = useState<Workspace[] | null>(null);
  const [error, setError] = useState("");
  const [tick, setTick] = useState(0);
  /** The workspace whose shell just went to the shells tab open already. */
  const [handed, setHanded] = useState<string | null>(null);

  useEffect(() => {
    if (!me?.user) return;
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
  }, [me, tick]);

  useEffect(() => {
    const again = () => document.visibilityState === "visible" && setTick((n) => n + 1);
    const t = setInterval(again, REFRESH_MS);
    document.addEventListener("visibilitychange", again);
    return () => {
      clearInterval(t);
      document.removeEventListener("visibilitychange", again);
    };
  }, []);

  if (!me?.user) return null;

  return (
    <Section>
      <div className="wrap max-w-[960px]">
        <div className="mb-6 flex flex-wrap items-baseline justify-between gap-3">
          <h1 className="text-xl">Your workspaces</h1>
          {list && list.length > 0 ? (
            <p className="text-xs text-muted-foreground" role="status">
              {handed
                ? `A shell in ${handed} is open in your shells tab.`
                : "Shells open in one tab of their own, logged in as you."}
            </p>
          ) : null}
        </div>
        {error ? <ErrorNote>{error}</ErrorNote> : null}
        {list === null && !error ? <p className="text-muted-foreground">Loading your workspaces…</p> : null}
        {list !== null && list.length === 0 ? <Empty /> : null}
        {list !== null && list.length > 0 ? (
          <List>
            {list.map((w) => (
              <WorkspaceRow key={`${w.cluster_id || w.host_id}/${w.name}`} workspace={w} onOpened={setHanded} />
            ))}
          </List>
        ) : null}
      </div>
    </Section>
  );
}

function WorkspaceRow({ workspace: w, onOpened }: { workspace: Workspace; onOpened: (handed: string | null) => void }) {
  const state = STATE[w.state] ?? STATE.unknown;
  const why = noShell(w);
  return (
    <li className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1 px-4 py-3">
      <Dot state={state.state} />
      <div className="min-w-0">
        <span className="font-mono text-[13px]">{w.name}</span>
        <span className="ml-2 truncate text-xs text-muted-foreground">
          {[w.user && `as ${w.user}`, `on ${w.host_name}`].filter(Boolean).join(" · ")}
        </span>
        <div className="truncate font-mono text-xs text-muted-foreground" title={w.image}>
          {w.image}
        </div>
        {why ? <div className="text-xs text-muted-foreground">{why}</div> : null}
      </div>
      <span className="flex items-center gap-2">
        <Pill state={state.state} className="hidden sm:inline-flex">
          {state.label}
        </Pill>
        {why ? (
          <Button variant="outline" size="sm" disabled className="gap-2">
            <SquareTerminal />
            Shell
          </Button>
        ) : (
          <Button asChild variant="outline" size="sm" className="gap-2">
            {/* Your shells page, in a tab of its own, loaded afresh: the terminal wants more of the
                browser than the rest does. When it's open already, the shell goes there. */}
            <a
              href={shellHref(w)}
              target={SHELLS_WINDOW}
              onClick={(e) => {
                if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
                e.preventDefault();
                void openShell(w).then((how) => onOpened(how === "handed" ? w.name : null));
              }}
            >
              <SquareTerminal />
              Shell
            </a>
          </Button>
        )}
      </span>
    </li>
  );
}

function Empty() {
  return (
    <div className="rounded-lg border border-dashed border-border-strong bg-card px-6 py-8">
      <Boxes className="mb-3 size-6 text-muted-foreground" strokeWidth={1.5} />
      <h2 className="mb-2 text-base">A shell in your workspaces, from any browser</h2>
      <p className="mb-4 max-w-[60ch] text-muted-foreground">
        Your dev containers on your own cluster show up here, with a terminal a click away. Nothing is opened to the internet:
        the usecode daemon on the cluster's controller calls out to us, and your shell rides that.
      </p>
      <ol className="list-decimal space-y-2 pl-5 text-muted-foreground">
        <li>
          Make a workspace, if you haven't yet: <code className="font-mono text-foreground">uc ws up</code>
        </li>
        <li>
          Have the controllers call home, as on the <a href="/hosts" className="text-primary hover:underline">hosts page</a>:{" "}
          <code className="font-mono text-foreground">uc daemon register --all</code>
        </li>
        <li>They report their workspaces within a minute.</li>
      </ol>
    </div>
  );
}
