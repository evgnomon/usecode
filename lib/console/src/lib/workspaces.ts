// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// Workspaces (`uc ws`), as GET /workspaces sends them, and the shell into
// one. No React here, so the tests can read it too.

export type WorkspaceState = "running" | "starting" | "stopped" | "unknown";

export type Workspace = {
  name: string;
  state: WorkspaceState;
  /** Who you are in there. */
  user: string;
  image: string;
  /** The controller that reports it and carries its shells. */
  host_id: string;
  host_name: string;
  cluster_id: string;
  /** The host is heard from. */
  online: boolean;
  /** The host's relay is open: a shell can be had. */
  connected: boolean;
  /** Seconds since the epoch. */
  last_seen_at: number;
};

export type WorkspaceList = { workspaces: Workspace[] };

/** The console's page with a shell in `w`. */
export function shellHref(w: Pick<Workspace, "host_id" | "name">): string {
  return `/shell?${new URLSearchParams({ host: w.host_id, ws: w.name })}`;
}

/** The API path that hands out a ticket for a shell in `name` on `host`. */
export function ticketPath(host: string, name: string): string {
  return `/workspaces/${encodeURIComponent(host)}/${encodeURIComponent(name)}/shell`;
}

/** The shell's WebSocket on the API at `api`, paid for with `ticket`. */
export function shellSocketUrl(api: string, ticket: string): string {
  const url = new URL("/workspaces/shell", api);
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.searchParams.set("ticket", ticket);
  return url.toString();
}

/** Why there's no shell to be had in `w` just now; null when there is. */
export function noShell(w: Workspace): string | null {
  if (w.state === "stopped") return `Stopped. \`uc ws start ${w.name}\` starts it.`;
  if (!w.online) return `${w.host_name} has gone quiet.`;
  if (!w.connected) return `${w.host_name} isn't connected for shells; \`uc daemon install ${w.host_name}\` brings its daemon up to date.`;
  return null;
}

/** What the page says once a shell ended. */
export function endedWith(code: number): string {
  return code === 0 ? "The shell exited." : `The shell exited with status ${code}.`;
}
