// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The console's pieces: status, nodes, options, activity. Everything here
// says what is wanted, what the agent is doing about it, or that it's done.

import { useEffect, useState, type ReactNode } from "react";
import { Server } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/progress";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

/** Desired: what was asked. Reconciling: the agent is on it. Settled: done. */
export type State = "desired" | "reconciling" | "settled" | "drift" | "failed";
export const STATES: readonly State[] = ["desired", "reconciling", "settled", "drift", "failed"];

const STATE_LABEL: Record<State, string> = {
  desired: "Desired",
  reconciling: "Reconciling",
  settled: "Settled",
  drift: "Drift",
  failed: "Failed",
};

const DOT: Record<State, string> = {
  desired: "border-[1.5px] border-muted-foreground bg-transparent",
  reconciling: "bg-primary animate-ring",
  settled: "bg-settled",
  drift: "bg-drift",
  failed: "bg-failed",
};

export function Dot({ state, className }: { state: State; className?: string }) {
  return <span aria-hidden="true" className={cn("inline-block size-2 shrink-0 rounded-full", DOT[state], className)} />;
}

const PILL: Record<State, string> = {
  desired: "border-dashed border-border-strong text-muted-foreground",
  reconciling: "border-primary/30 bg-agent-soft text-primary",
  settled: "border-transparent bg-settled-soft text-settled",
  drift: "border-drift/35 bg-drift-soft text-drift",
  failed: "border-failed/35 bg-failed-soft text-failed",
};

export function Pill({ state, children, className }: { state: State; children?: ReactNode; className?: string }) {
  return (
    <Badge variant="outline" className={cn("h-[22px] gap-1.5", PILL[state], className)}>
      <Dot state={state} className={cn("size-1.5", state === "desired" && "border")} />
      {children ?? STATE_LABEL[state]}
    </Badge>
  );
}

/** The agent across the whole cluster: idle, or working on some tasks. */
export function AgentStatus({ tasks }: { tasks: number }) {
  const working = tasks > 0;
  return (
    <span role="status" className={cn("inline-flex items-center gap-2 text-xs", working ? "text-foreground" : "text-muted-foreground")}>
      <Dot state={working ? "reconciling" : "settled"} className="size-1.5" />
      {working ? `Working · ${tasks} ${tasks === 1 ? "task" : "tasks"}` : "Idle, all settled"}
    </span>
  );
}

/** Rows of nodes or options, in one bordered box. */
export function List({ children }: { children: ReactNode }) {
  return <ul className="divide-y overflow-hidden rounded-lg border bg-card">{children}</ul>;
}

export type Node = {
  name: string;
  role: string;
  resources: string;
  state: State;
  label?: string;
  /** What the agent is doing to it right now, step `at` of `of`. */
  step?: { text: string; at: number; of: number };
};

export function NodeRow({ node: n }: { node: Node }) {
  return (
    <li
      className={cn(
        "grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-x-4 gap-y-2 px-4 py-3 sm:grid-cols-[auto_minmax(0,1fr)_auto_auto]",
        n.state === "reconciling" && "animate-breathe-bg",
        n.state === "drift" && "bg-drift-soft",
      )}
    >
      <Dot state={n.state} />
      <div>
        <span className={cn("font-mono text-[13px]", n.state === "reconciling" && "text-primary")}>{n.name}</span>
        <span className="ml-2 text-xs text-muted-foreground">{n.role}</span>
      </div>
      <span className="col-start-2 row-start-2 font-mono text-xs whitespace-nowrap text-muted-foreground sm:col-start-auto sm:row-start-auto">
        {n.resources}
      </span>
      <Pill state={n.state} className="col-start-3 row-start-1 sm:col-start-auto sm:row-start-auto">
        {n.label}
      </Pill>
      {n.step ? (
        <div className="col-span-full col-start-2 flex items-center gap-3 font-mono text-xs text-muted-foreground">
          <Progress
            value={(n.step.at / n.step.of) * 100}
            aria-label={`${n.name}: step ${n.step.at} of ${n.step.of}`}
            className="h-0.5 max-w-56 flex-1 bg-border"
          />
          <span>
            {n.step.text} {n.step.at}/{n.step.of}
          </span>
        </div>
      ) : null}
    </li>
  );
}

export type Option = {
  name: string;
  hint: string;
  on: boolean;
  state: State;
  /** What the state reads as, e.g. "applying…" or "queued". */
  says?: string;
};

function OptionText({ name, hint }: { name: string; hint: string }) {
  return (
    <div>
      <div className="font-medium">{name}</div>
      <div className="text-xs text-muted-foreground">{hint}</div>
    </div>
  );
}

const OPTION_ROW = "grid grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-x-4 gap-y-3 px-4 py-3";

/** An option as the agent sees it. The switch shows the wish, the words what's real. */
export function OptionRow({ option: o }: { option: Option }) {
  const says = o.says ?? (o.state === "reconciling" ? "applying…" : o.on ? "On" : "Off");
  return (
    <li className={cn(OPTION_ROW, o.state === "reconciling" && "animate-breathe-bg")}>
      <OptionText name={o.name} hint={o.hint} />
      <span
        className={cn(
          "min-w-18 text-right text-xs text-muted-foreground",
          o.state === "reconciling" && "animate-breathe text-primary",
          o.state === "drift" && "text-drift",
        )}
      >
        {says}
      </span>
      <Switch
        checked={o.on}
        tabIndex={-1}
        aria-label={`${o.name}: ${o.on ? "on" : "off"}, ${says}`}
        className={cn(
          "pointer-events-none",
          o.state === "reconciling" && "animate-breathe data-[state=checked]:bg-primary/55",
          o.state === "desired" && "outline-1 outline-offset-2 outline-primary outline-dashed",
        )}
      />
    </li>
  );
}

/** A real switch: flip it on and it says "applying…" for a moment before it settles. */
export function OptionSwitch({ name, hint }: { name: string; hint: string }) {
  const [phase, setPhase] = useState<"off" | "applying" | "on">("off");
  useEffect(() => {
    if (phase !== "applying") return;
    const t = setTimeout(() => setPhase("on"), 1800);
    return () => clearTimeout(t);
  }, [phase]);
  return (
    <li>
      <label className={cn(OPTION_ROW, "cursor-pointer", phase === "applying" && "animate-breathe-bg")}>
        <OptionText name={name} hint={hint} />
        <span
          aria-live="polite"
          className={cn(
            "min-w-18 text-right text-xs text-muted-foreground",
            phase === "applying" && "animate-breathe text-primary",
            phase === "on" && "text-settled",
          )}
        >
          {phase === "applying" ? "applying…" : phase === "on" ? "On" : "Off"}
        </span>
        <Switch checked={phase !== "off"} onCheckedChange={(on) => setPhase(on ? "applying" : "off")} />
      </label>
    </li>
  );
}

export type Line = { at: string; text: string; state?: State };

const LINE_DOT: Record<State, string> = {
  desired: "bg-border-strong",
  reconciling: "bg-primary animate-breathe",
  settled: "bg-settled",
  drift: "bg-drift",
  failed: "bg-failed",
};

/** What the agent is doing, newest first, in its own quiet voice. */
export function Activity({ lines }: { lines: Line[] }) {
  return (
    <ol aria-label="Activity" className="rounded-lg border bg-card px-4 py-3 font-mono text-xs leading-normal">
      {lines.map((l, i) => (
        <li
          key={`${l.at} ${l.text}`}
          className={cn(
            "grid grid-cols-[auto_auto_minmax(0,1fr)] gap-3 py-[3px] text-muted-foreground",
            (i === 0 || l.state === "reconciling") && "text-foreground",
            i === 0 && "animate-feed-in",
          )}
        >
          <span aria-hidden="true" className={cn("mt-1.5 size-1.5 rounded-full", LINE_DOT[l.state ?? "desired"])} />
          <time className="text-muted-foreground/85">{l.at}</time>
          <span>{l.text}</span>
        </li>
      ))}
    </ol>
  );
}

export function EmptyNodes({ children }: { children?: ReactNode }) {
  return (
    <div className="rounded-lg border border-dashed border-border-strong bg-card px-6 py-8 text-center">
      <Server className="mx-auto mb-3 size-6 text-muted-foreground" strokeWidth={1.5} />
      <h3 className="mb-2 text-base">Add a node, we'll handle the rest</h3>
      <p className="mx-auto mb-4 max-w-[44ch] text-muted-foreground last:mb-0">
        Any Linux box, VM or Kubernetes node. It joins your mesh, gets set up and shows up here, settled.
      </p>
      {children}
    </div>
  );
}
