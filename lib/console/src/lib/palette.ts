// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// What the command palette (Ctrl+Shift+P) matches what you type against, as
// an editor's does: the letters in order, not necessarily side by side, so
// "wsp" finds "Workspaces". A name starting with it ranks first, then one
// with it whole somewhere inside, then the rest.

/** One line of the palette: what it reads as, and other words it answers to. */
export type Entry = { label: string; keywords?: string };

/**
 * How well `query` matches `text`, lower is better; null when it doesn't.
 * Case doesn't matter, nor do spaces in the query.
 */
export function score(text: string, query: string): number | null {
  const t = text.toLowerCase();
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return 0;
  if (t.startsWith(q)) return 0;
  const at = t.indexOf(q);
  if (at >= 0) return 1 + at / 100;
  // In order with gaps: the fewer letters skipped, the better.
  let i = 0;
  let skipped = 0;
  for (const c of t) {
    if (i < q.length && c === q[i]) i++;
    else if (i > 0 && i < q.length) skipped++;
  }
  return i === q.length ? 2 + skipped / 100 : null;
}

/** The entries `query` matches, the best first; ties keep their order. */
export function filter<T extends Entry>(entries: T[], query: string): T[] {
  return entries
    .map((e, n) => {
      const byLabel = score(e.label, query);
      const byWords = e.keywords ? score(e.keywords, query) : null;
      // A label beats keywords that match just as well.
      const s = byLabel ?? (byWords === null ? null : byWords + 0.5);
      return { e, n, s };
    })
    .filter((x): x is { e: T; n: number; s: number } => x.s !== null)
    .sort((a, b) => a.s - b.s || a.n - b.n)
    .map((x) => x.e);
}

/** Whether a keydown opens the palette: Ctrl+Shift+P, or Cmd+Shift+P on a Mac. */
export const opensPalette = (e: { key: string; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean }) =>
  e.shiftKey && !e.altKey && (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "p";
