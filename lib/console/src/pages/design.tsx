// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The console's theme on one page: its tokens in both themes, its pieces,
// and the screens they make. The cluster here is made up.

import type { ReactNode } from "react";
import { Plus, TriangleAlert } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Activity,
  AgentStatus,
  EmptyNodes,
  List,
  NodeRow,
  OptionRow,
  OptionSwitch,
  Pill,
  STATES,
  type Line,
  type Node,
  type Option,
} from "@/components/console";
import { PageHead, Section, ThemeSwitch } from "@/components/layout";
import { useTitle } from "@/lib/router";
import { cn } from "@/lib/utils";

// Spelled out so Tailwind sees every class.
const COLORS = [
  ["background", "bg-background"],
  ["card", "bg-card"],
  ["muted", "bg-muted"],
  ["border", "bg-border"],
  ["foreground", "bg-foreground"],
  ["muted-foreground", "bg-muted-foreground"],
  ["primary", "bg-primary"],
  ["settled", "bg-settled"],
  ["drift", "bg-drift"],
  ["failed", "bg-failed"],
] as const;
const SPACE = [
  [1, "w-1"],
  [2, "w-2"],
  [3, "w-3"],
  [4, "w-4"],
  [6, "w-6"],
  [8, "w-8"],
  [12, "w-12"],
  [16, "w-16"],
] as const;
const TYPE = [
  { size: 20, cls: "text-xl", sample: "Cluster overview" },
  { size: 16, cls: "text-base", sample: "Add a node, we'll handle the rest" },
  { size: 14, cls: "text-sm", sample: "Ingress routes traffic to your services" },
  { size: 13, cls: "font-mono text-[13px]", sample: "node-04 · 10.8.0.4 · v1.31.4" },
  { size: 12, cls: "font-mono text-xs", sample: "14:02:31 Joining node-04 to the mesh…" },
];

const settled = (name: string, role: string, resources: string): Node => ({ name, role, resources, state: "settled" });
const NODES: Node[] = [
  settled("node-01", "control plane", "4 vCPU · 8 GiB · 10.8.0.1"),
  settled("node-02", "worker", "8 vCPU · 16 GiB · 10.8.0.2"),
  settled("node-03", "worker", "8 vCPU · 16 GiB · 10.8.0.3"),
];
const JOINING: Node = {
  name: "node-04",
  role: "worker",
  resources: "8 vCPU · 16 GiB · 10.8.0.4",
  state: "reconciling",
  label: "Joining",
  step: { text: "Installing kubelet…", at: 3, of: 5 },
};
const DRIFTED: Node = { ...NODES[1]!, state: "drift", label: "Drift" };

const OPTIONS: Option[] = [
  { name: "Private mesh", hint: "WireGuard between all nodes", on: true, state: "settled" },
  { name: "Ingress", hint: "Routes and TLS for your services", on: true, state: "settled" },
  { name: "Metrics and logs", hint: "Collected and kept for 30 days", on: true, state: "reconciling" },
  { name: "Backups", hint: "Volumes, every night", on: true, state: "desired", says: "queued" },
  { name: "Automatic upgrades", hint: "Patch releases, one node at a time", on: false, state: "settled" },
];

const FEED: Line[] = [
  { at: "14:02:31", text: "Joining node-04 to the cluster…", state: "reconciling" },
  { at: "14:02:18", text: "Enabling metrics, deploying the collector…", state: "reconciling" },
  { at: "14:02:02", text: "Backups on, queued behind metrics", state: "desired" },
  { at: "14:01:55", text: "node-04 joined the mesh as 10.8.0.4", state: "settled" },
  { at: "14:01:40", text: "Ingress enabled, routes applied", state: "settled" },
  { at: "13:58:02", text: "All 3 nodes match what you asked for", state: "settled" },
];

function Screen({ cluster, tasks, children }: { cluster: string; tasks: number; children: ReactNode }) {
  return (
    <div className="overflow-hidden rounded-lg border bg-background">
      <div className="flex min-h-11 flex-wrap items-center gap-4 border-b bg-card px-4 py-2">
        <span className="font-mono text-[13px]">{cluster}</span>
        <AgentStatus tasks={tasks} />
        <span className="flex-1" />
        <ThemeSwitch />
      </div>
      <div className="grid gap-5 p-3 sm:p-5">{children}</div>
    </div>
  );
}

function Caption({ title, note }: { title: string; note: string }) {
  return (
    <div className="mt-8 mb-3 flex flex-col justify-between gap-1 sm:flex-row sm:items-baseline sm:gap-3">
      <h2 className="text-base">{title}</h2>
      <p className="text-[13px] text-muted-foreground">{note}</p>
    </div>
  );
}

function Panel({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card className="gap-0 rounded-lg py-5 shadow-none">
      <CardContent className="grid gap-3 px-5">
        <h3>{title}</h3>
        {children}
      </CardContent>
    </Card>
  );
}

function Tokens() {
  return (
    <>
      <Caption title="Tokens" note="One set, two themes. Every colour below is a variable in src/index.css." />
      <div className="grid gap-4 md:grid-cols-2">
        {(["light", "dark"] as const).map((t) => (
          <div key={t} className={cn(t, "grid content-start gap-3 rounded-lg border p-4")}>
            <p className="text-xs text-muted-foreground">{t}</p>
            <ul className="grid grid-cols-[repeat(auto-fill,minmax(150px,1fr))] gap-2">
              {COLORS.map(([name, cls]) => (
                <li key={name} className="flex items-center gap-2 font-mono text-xs text-muted-foreground">
                  <span className={cn("size-5 shrink-0 rounded-md border", cls)} />--{name}
                </li>
              ))}
            </ul>
            <div className="flex flex-wrap gap-3">
              {STATES.map((s) => (
                <Pill key={s} state={s} />
              ))}
            </div>
          </div>
        ))}
      </div>
      <div className="mt-4 grid gap-4 md:grid-cols-2">
        <Panel title="Spacing, a 4px grid">
          <ul className="grid gap-2">
            {SPACE.map(([n, cls]) => (
              <li key={n} className="grid grid-cols-[72px_minmax(0,1fr)] items-center gap-3 font-mono text-xs text-muted-foreground">
                {n * 4}px <span className={cn("h-2 rounded-[2px] border border-primary/30 bg-agent-soft", cls)} />
              </li>
            ))}
          </ul>
        </Panel>
        <Panel title="Radius and motion">
          <div className="flex flex-wrap items-center gap-3">
            <span className="h-7 w-10 rounded-md border border-border-strong bg-card" />
            <small className="font-mono text-xs text-muted-foreground">6px</small>
            <span className="h-7 w-10 rounded-lg border border-border-strong bg-card" />
            <small className="font-mono text-xs text-muted-foreground">8px</small>
            <span className="h-7 w-10 rounded-full border border-border-strong bg-card" />
            <small className="font-mono text-xs text-muted-foreground">pill</small>
          </div>
          <p className="text-muted-foreground">
            150 to 250ms, ease-out. A 2s breath only on what the agent is working on. Reduced motion keeps the words and
            drops the movement.
          </p>
        </Panel>
      </div>
      <div className="mt-4">
        <Panel title="Type">
          <ul className="grid gap-2">
            {TYPE.map((t) => (
              <li key={t.size} className="grid grid-cols-[72px_minmax(0,1fr)] items-center gap-3 font-mono text-xs text-muted-foreground">
                {t.size}px
                <span className={cn("text-foreground", t.cls)}>{t.sample}</span>
              </li>
            ))}
          </ul>
        </Panel>
      </div>
    </>
  );
}

function AddNode({ id }: { id?: string }) {
  return (
    <Button asChild size="sm" className="h-7 text-xs">
      <a href="#add" id={id}>
        <Plus />
        Add node
      </a>
    </Button>
  );
}

export function Design() {
  useTitle("Theme · usecode.dev");
  return (
    <>
      <PageHead title="Declare, then delegate">
        The console's theme. You add nodes and flip options; the agent does the rest, and the page shows three things:
        what you asked for, what it's doing about it, and what's settled.
      </PageHead>
      <Section>
        <div className="wrap">
          <Caption title="Cluster overview" note="Nodes, options and what the agent is up to." />
          <Screen cluster="prod-eu-1" tasks={2}>
            <div className="grid items-start gap-5 md:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]">
              <div className="grid content-start gap-3">
                <div className="flex items-center justify-between gap-3">
                  <h3>Nodes</h3>
                  <AddNode />
                </div>
                <List>
                  {[...NODES, JOINING].map((n) => (
                    <NodeRow key={n.name} node={n} />
                  ))}
                </List>
                <h3>Options</h3>
                <List>
                  {OPTIONS.map((o) => (
                    <OptionRow key={o.name} option={o} />
                  ))}
                </List>
              </div>
              <div className="grid content-start gap-3">
                <h3>Activity</h3>
                <Activity lines={FEED} />
              </div>
            </div>
          </Screen>

          <Caption title="A node being added" note="It breathes while it joins, then settles like the rest." />
          <Screen cluster="prod-eu-1" tasks={1}>
            <List>
              <NodeRow node={NODES[0]!} />
              <NodeRow node={JOINING} />
            </List>
            <Activity
              lines={[
                { at: "14:02:31", text: "Installing kubelet on node-04…", state: "reconciling" },
                { at: "14:01:55", text: "node-04 joined the mesh as 10.8.0.4", state: "settled" },
                { at: "14:01:49", text: "node-04 checked in, 8 vCPU, 16 GiB", state: "settled" },
                { at: "14:01:48", text: "You added node-04", state: "desired" },
              ]}
            />
          </Screen>

          <Caption title="An option being applied" note="Flip one on: it says applying… before it settles." />
          <Screen cluster="prod-eu-1" tasks={1}>
            <List>
              <OptionRow option={OPTIONS[2]!} />
              <OptionSwitch name="Try it: GPU scheduling" hint="Flip it on and watch the hand-off" />
              <OptionRow option={OPTIONS[1]!} />
            </List>
          </Screen>

          <Caption title="Drift" note="Rare, plain and amber. The agent says what it found and what it's doing." />
          <Screen cluster="prod-eu-1" tasks={1}>
            <div role="status" className="flex items-start gap-3 rounded-md border border-drift/30 bg-drift-soft p-3">
              <TriangleAlert className="mt-0.5 size-4 shrink-0 text-drift" strokeWidth={1.5} />
              <p>
                <span className="font-mono">node-02</span> runs kubelet <span className="font-mono">1.31.2</span>, you
                asked for <span className="font-mono">1.31.4</span>. Putting it back, nothing for you to do.
              </p>
            </div>
            <List>
              <NodeRow node={NODES[0]!} />
              <NodeRow node={DRIFTED} />
              <NodeRow node={NODES[2]!} />
            </List>
            <Activity
              lines={[
                { at: "14:07:12", text: "Draining node-02 to upgrade kubelet…", state: "reconciling" },
                { at: "14:07:10", text: "node-02 drifted: kubelet 1.31.2, want 1.31.4", state: "drift" },
                { at: "13:58:02", text: "All 3 nodes match what you asked for", state: "settled" },
              ]}
            />
          </Screen>

          <Caption title="No nodes yet" note="The one thing to do, and the promise behind it." />
          <Screen cluster="new-cluster" tasks={0}>
            <EmptyNodes>
              <AddNode id="add" />
            </EmptyNodes>
          </Screen>

          <Tokens />
        </div>
      </Section>
    </>
  );
}
