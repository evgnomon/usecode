// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Your hosts: every machine running the usecode daemon that calls home
// (`uc daemon register`). Made for many: the API pages, filters and
// searches, so a thousand hosts are fifty rows at a time, and the counts
// on top say where to look first.

import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, ChevronRight, Copy, Search, Server } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dot, List, Pill, type State } from "@/components/console";
import { KeyLegend } from "@/components/kbd";
import { ErrorNote, Section } from "@/components/layout";
import { del, get, post, useMe } from "@/lib/api";
import {
  ago,
  duration,
  HOST_FILTERS,
  hostHealth,
  hostsPath,
  pageRange,
  type Host,
  type HostFilter,
  type HostList,
  type HostSort,
} from "@/lib/hosts";
import { navigate } from "@/lib/router";
import { cn } from "@/lib/utils";

const PAGE = 50;
/** How often the list looks again while the page is in view; hosts call every five minutes. */
const REFRESH_MS = 30_000;

const HEALTH: Record<ReturnType<typeof hostHealth>, { state: State; label: string }> = {
  online: { state: "settled", label: "Online" },
  failing: { state: "failed", label: "Failing" },
  offline: { state: "drift", label: "Offline" },
};

export function Hosts() {
  const { me } = useMe();
  useEffect(() => {
    if (me && !me.user) navigate("/login?next=/hosts", { replace: true });
  }, [me]);

  const [typed, setTyped] = useState("");
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<HostFilter>("all");
  const [sort, setSort] = useState<HostSort>("name");
  const [offset, setOffset] = useState(0);
  const [tick, setTick] = useState(0);
  const [list, setList] = useState<HostList | null>(null);
  const [error, setError] = useState("");
  const searchBox = useRef<HTMLInputElement>(null);

  // Search once typing pauses, from the first page.
  useEffect(() => {
    const t = setTimeout(() => {
      setSearch(typed);
      setOffset(0);
    }, 250);
    return () => clearTimeout(t);
  }, [typed]);

  useEffect(() => {
    if (!me?.user) return;
    const ctl = new AbortController();
    get<HostList>(hostsPath({ search, filter, sort, offset, limit: PAGE }), ctl.signal)
      .then((l) => {
        setList(l);
        setError("");
      })
      .catch((err: Error) => {
        if (!ctl.signal.aborted) setError(err.message);
      });
    return () => ctl.abort();
  }, [me, search, filter, sort, offset, tick]);

  // Look again now and then, and when the tab comes back into view.
  useEffect(() => {
    const again = () => document.visibilityState === "visible" && setTick((n) => n + 1);
    const t = setInterval(again, REFRESH_MS);
    document.addEventListener("visibilitychange", again);
    return () => {
      clearInterval(t);
      document.removeEventListener("visibilitychange", again);
    };
  }, []);

  // "/" jumps to the search, as on most sites with one.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "/" || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.target instanceof HTMLElement && (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA")) return;
      e.preventDefault();
      searchBox.current?.focus();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!me?.user) return null;
  const counts = list?.counts;
  const none = list !== null && list.counts.all === 0 && !search;

  const pick = (f: HostFilter) => {
    setFilter(f);
    setOffset(0);
  };

  return (
    <Section>
      <div className="wrap max-w-[960px]">
        <div className="mb-6 flex flex-wrap items-baseline justify-between gap-3">
          <h1 className="text-xl">Your hosts</h1>
          {counts && counts.all > 0 ? (
            <p className="text-xs text-muted-foreground">
              Each host checks in every five minutes; this page looks again every 30 seconds.
            </p>
          ) : null}
        </div>
        {error ? <ErrorNote>{error}</ErrorNote> : null}
        {list === null && !error ? <p className="text-muted-foreground">Loading your hosts…</p> : null}
        {none ? <Connect first /> : null}
        {list !== null && !none ? (
          <>
            <div className="mb-3 flex flex-wrap items-center gap-2">
              <div role="group" aria-label="Show" className="flex flex-wrap gap-1">
                {HOST_FILTERS.map((f) => (
                  <Button
                    key={f.key}
                    variant={filter === f.key ? "secondary" : "ghost"}
                    size="sm"
                    aria-pressed={filter === f.key}
                    onClick={() => pick(f.key)}
                    className="gap-2"
                  >
                    {f.key === "all" ? null : <Dot state={HEALTH[f.key].state} className="size-1.5" />}
                    {f.label}
                    <span className="font-mono text-xs text-muted-foreground">{(counts?.[f.key] ?? 0).toLocaleString("en-US")}</span>
                  </Button>
                ))}
              </div>
              <div className="relative ml-auto w-full sm:w-64">
                <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  ref={searchBox}
                  type="search"
                  value={typed}
                  onChange={(e) => setTyped(e.target.value)}
                  placeholder="Name, OS, version or IP"
                  aria-label="Search hosts"
                  aria-keyshortcuts="/"
                  className="h-8 pr-8 pl-8"
                />
                <KeyLegend k="/" className="absolute top-1/2 right-2.5 -translate-y-1/2" />
              </div>
            </div>

            <div className="mb-2 flex items-center justify-between gap-3 text-xs text-muted-foreground">
              <span aria-live="polite">{pageRange(list.offset, list.hosts.length, list.total)}</span>
              <label className="flex items-center gap-2">
                Sort
                <select
                  value={sort}
                  onChange={(e) => {
                    setSort(e.target.value as HostSort);
                    setOffset(0);
                  }}
                  className="h-7 rounded-md border bg-background px-2 text-xs text-foreground"
                >
                  <option value="name">by name</option>
                  <option value="quiet">quiet the longest first</option>
                </select>
              </label>
            </div>

            {list.hosts.length === 0 ? (
              <p className="rounded-lg border border-dashed px-4 py-6 text-center text-muted-foreground">
                {search ? `No host matches “${search}”.` : "None here, which is good news."}
              </p>
            ) : (
              <List>
                {list.hosts.map((h) => (
                  <HostRow key={h.id} host={h} onRemoved={() => setTick((n) => n + 1)} />
                ))}
              </List>
            )}

            {list.total > PAGE ? (
              <div className="mt-3 flex items-center justify-end gap-2">
                <Button variant="outline" size="sm" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>
                  Previous
                </Button>
                <Button variant="outline" size="sm" disabled={offset + PAGE >= list.total} onClick={() => setOffset(offset + PAGE)}>
                  Next
                </Button>
              </div>
            ) : null}

            <details className="mt-8 rounded-lg border bg-card px-4 py-3">
              <summary className="cursor-pointer">Add more hosts</summary>
              <div className="pt-3">
                <Connect />
              </div>
            </details>
          </>
        ) : null}
      </div>
    </Section>
  );
}

function HostRow({ host: h, onRemoved }: { host: Host; onRemoved: () => void }) {
  const [open, setOpen] = useState(false);
  const [sure, setSure] = useState(false);
  const [error, setError] = useState("");
  const health = HEALTH[hostHealth(h)];
  const broken = h.modules.filter((m) => m.status !== "ok");
  const Chevron = open ? ChevronDown : ChevronRight;

  const remove = async () => {
    if (!sure) return setSure(true);
    try {
      await del(`/hosts/${h.id}`);
      onRemoved();
    } catch (err) {
      setError((err as Error).message);
    }
  };

  return (
    <li className={cn(h.failing && "bg-failed-soft", !h.online && "bg-drift-soft")}>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="grid w-full cursor-pointer grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1 px-4 py-3 text-left sm:grid-cols-[auto_minmax(0,1fr)_8rem_7rem_auto]"
      >
        <Dot state={health.state} />
        <div className="min-w-0">
          <span className="font-mono text-[13px]">{h.name}</span>
          <span className="ml-2 truncate text-xs text-muted-foreground">
            {[h.os, h.arch].filter(Boolean).join(" · ")}
          </span>
          {broken.length > 0 && h.online ? (
            <div className="truncate font-mono text-xs text-failed">
              {broken.map((m) => `${m.name}: ${m.detail ?? m.status}`).join(" · ")}
            </div>
          ) : null}
        </div>
        <span className="hidden font-mono text-xs text-muted-foreground sm:block">{h.version ? `v${h.version}` : ""}</span>
        <time
          dateTime={new Date(h.last_seen_at * 1000).toISOString()}
          title={new Date(h.last_seen_at * 1000).toLocaleString()}
          className="hidden text-xs text-muted-foreground sm:block"
        >
          {ago(h.last_seen_at)}
        </time>
        <span className="flex items-center gap-2">
          <Pill state={health.state}>{health.label}</Pill>
          <Chevron className="size-4 text-muted-foreground" />
        </span>
      </button>
      {open ? (
        <div className="grid gap-4 border-t border-dashed px-4 py-3 pl-10 text-xs sm:grid-cols-[minmax(0,1fr)_auto]">
          <div>
            <dl className="mb-3 grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1">
              <dt className="text-muted-foreground">Last heard</dt>
              <dd>{ago(h.last_seen_at)}</dd>
              <dt className="text-muted-foreground">Up for</dt>
              <dd>{h.uptime_seconds === null ? "—" : duration(h.uptime_seconds)}</dd>
              <dt className="text-muted-foreground">Calls from</dt>
              <dd className="font-mono">{h.last_ip ?? "—"}</dd>
              <dt className="text-muted-foreground">Daemon</dt>
              <dd className="font-mono">{h.version ? `usecoded ${h.version}` : "—"}</dd>
              <dt className="text-muted-foreground">Registered</dt>
              <dd>{new Date(h.created_at * 1000).toLocaleDateString()}</dd>
            </dl>
            {h.modules.length > 0 ? (
              <ul className="space-y-1 font-mono">
                {h.modules.map((m) => (
                  <li key={m.name} className="flex gap-2">
                    <Dot state={m.status === "ok" ? "settled" : "failed"} className="mt-1.5 size-1.5" />
                    <span>
                      {m.name}: <span className={m.status === "ok" ? "text-muted-foreground" : "text-failed"}>{m.detail ?? m.status}</span>
                    </span>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-muted-foreground">No module has reported yet.</p>
            )}
            {!h.online ? (
              <p className="mt-3 text-muted-foreground">
                Gone quiet? Check the daemon there: <code className="font-mono">journalctl -u usecode</code>
              </p>
            ) : null}
          </div>
          <div className="flex flex-col items-start gap-2 sm:items-end">
            {error ? <ErrorNote>{error}</ErrorNote> : null}
            <Button variant={sure ? "destructive" : "outline"} size="sm" onClick={remove} onBlur={() => setSure(false)}>
              {sure ? "Remove it for good" : "Remove"}
            </Button>
            {sure ? <span className="max-w-[28ch] text-muted-foreground sm:text-right">It stops calling home until you register it again.</span> : null}
          </div>
        </div>
      ) : null}
    </li>
  );
}

/** How to get hosts here: an API key, and one command on your machine. */
function Connect({ first = false }: { first?: boolean }) {
  const [key, setKey] = useState("");
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  const command = `export UC_USECODE_API_KEY=${key || "<your API key>"}\nuc daemon register --all`;

  const makeKey = async () => {
    try {
      const made = await post<{ api_key: string }>("/auth/api-keys", { label: "uc daemon register" });
      setKey(made.api_key);
    } catch (err) {
      setError((err as Error).message);
    }
  };
  const copy = async () => {
    await navigator.clipboard.writeText(command);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className={cn(first && "rounded-lg border border-dashed border-border-strong bg-card px-6 py-8")}>
      {first ? (
        <>
          <Server className="mb-3 size-6 text-muted-foreground" strokeWidth={1.5} />
          <h2 className="mb-2 text-base">See all your hosts in one place</h2>
          <p className="mb-4 max-w-[60ch] text-muted-foreground">
            Every box running the usecode daemon can check in here: whether it's up, which version it runs, and how its mesh,
            firewall and Kubernetes are doing. A thousand of them is fine.
          </p>
        </>
      ) : null}
      <ol className="mb-4 list-decimal space-y-2 pl-5 text-muted-foreground">
        <li>
          Put the daemon on your hosts, if it isn't there yet: <code className="font-mono text-foreground">uc daemon install NAME</code>
        </li>
        <li>Make an API key for it. It stays on your machine; each host gets a one-time token instead.</li>
        <li>Run this where you run uc. Hosts show up within a minute.</li>
      </ol>
      {error ? <ErrorNote>{error}</ErrorNote> : null}
      <div className="relative mb-3 rounded-lg border bg-background px-4 py-3 pr-12 font-mono text-xs break-all whitespace-pre-wrap">
        {command}
        <Button variant="ghost" size="icon-xs" aria-label="Copy the commands" onClick={copy} className="absolute top-2 right-2">
          {copied ? <Check /> : <Copy />}
        </Button>
      </div>
      {key ? (
        <p className="text-xs text-muted-foreground">That key is only shown this once; keep it somewhere safe.</p>
      ) : (
        <Button variant={first ? "default" : "outline"} size="sm" onClick={makeKey}>
          Make an API key
        </Button>
      )}
    </div>
  );
}
