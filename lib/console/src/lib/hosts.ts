// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Hosts calling home (lib/daemon's callhome module), as GET /hosts sends
// them, and the words the hosts page says them with. No React here, so the
// tests can read it too.

export type HostModule = { name: string; status: string; detail?: string };

export type Host = {
  id: string;
  name: string;
  version: string;
  arch: string;
  os: string;
  modules: HostModule[];
  uptime_seconds: number | null;
  last_ip: string | null;
  online: boolean;
  /** Online, with a module that hit an error. */
  failing: boolean;
  /** Seconds since the epoch. */
  last_seen_at: number;
  created_at: number;
};

export type HostFilter = "all" | "online" | "offline" | "failing";
export type HostSort = "name" | "quiet";
export type HostCounts = Record<HostFilter, number>;

export type HostList = { hosts: Host[]; total: number; counts: HostCounts; limit: number; offset: number };

export const HOST_FILTERS: { key: HostFilter; label: string }[] = [
  { key: "all", label: "All" },
  { key: "online", label: "Online" },
  { key: "failing", label: "Failing" },
  { key: "offline", label: "Offline" },
];

/** The API path for one page of hosts. */
export function hostsPath(q: { search: string; filter: HostFilter; sort: HostSort; offset: number; limit: number }): string {
  const params = new URLSearchParams();
  if (q.search.trim()) params.set("q", q.search.trim());
  if (q.filter !== "all") params.set("state", q.filter);
  if (q.sort !== "name") params.set("sort", q.sort);
  if (q.offset > 0) params.set("offset", String(q.offset));
  params.set("limit", String(q.limit));
  return `/hosts?${params}`;
}

/** Online and fine, online with a module in trouble, or gone quiet. */
export function hostHealth(h: Host): "online" | "failing" | "offline" {
  return !h.online ? "offline" : h.failing ? "failing" : "online";
}

/** "just now", "4 min ago", "3 h ago", "2 d ago", from seconds since the epoch. */
export function ago(at: number, now = Date.now() / 1000): string {
  const s = Math.max(0, Math.round(now - at));
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.max(1, Math.round(s / 60))} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return `${Math.floor(s / 86400)} d ago`;
}

/** "5 min", "3 h 20 min", "12 d 4 h". */
export function duration(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return h > 0 ? `${d} d ${h} h` : `${d} d`;
  if (h > 0) return m > 0 ? `${h} h ${m} min` : `${h} h`;
  return `${m} min`;
}

/** "1–50 of 1,000". */
export function pageRange(offset: number, shown: number, total: number): string {
  const n = (v: number) => v.toLocaleString("en-US");
  if (total === 0) return "none";
  return `${n(offset + 1)}–${n(offset + shown)} of ${n(total)}`;
}
