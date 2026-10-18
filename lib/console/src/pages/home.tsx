// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { ChevronDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { KeyLegend } from "@/components/kbd";
import { Activity, type Line } from "@/components/console";
import { Journeys } from "@/components/journeys";
import { Section } from "@/components/layout";
import { FAQS, FEATURES, MOTIVATION, REPO_URL, STEPS } from "@/content";
import { useMe } from "@/lib/api";
import { Link } from "@/lib/router";

// What the agent sounds like while it works, newest first.
const HERO_FEED: Line[] = [
  { at: "14:02:31", text: "Joining node-04 to the mesh…", state: "reconciling" },
  { at: "14:02:09", text: "Ingress enabled, routes applied", state: "settled" },
  { at: "13:58:44", text: "node-02 drifted, kubelet put back", state: "settled" },
  { at: "13:41:17", text: "All 3 nodes match what you asked for", state: "settled" },
];

export function Faqs() {
  return (
    <Section>
      <div className="wrap max-w-[680px]">
        <h2 className="mb-3 text-base">Questions</h2>
        {/* <details> rather than an accordion, so the answers are in the page's HTML for anything reading it. */}
        {FAQS.map((f) => (
          <details key={f.q} className="group border-b">
            <summary className="flex cursor-pointer list-none items-start justify-between gap-4 rounded-md py-4 text-sm font-medium outline-none hover:underline focus-visible:ring-[3px] focus-visible:ring-ring/50 [&::-webkit-details-marker]:hidden">
              {f.q}
              <ChevronDown className="pointer-events-none size-4 shrink-0 translate-y-0.5 text-muted-foreground transition-transform duration-200 group-open:rotate-180" />
            </summary>
            <p className="pb-4 text-sm text-muted-foreground">{f.a}</p>
          </details>
        ))}
      </div>
    </Section>
  );
}

export function Home() {
  const { me } = useMe();
  return (
    <>
      <section className="pt-14 pb-10 text-center sm:pt-24 sm:pb-16">
        <div className="wrap max-w-[760px]">
          <h1 className="mb-8 text-[clamp(28px,5.5vw,44px)] leading-[1.15] font-semibold tracking-[-0.02em]">
            {MOTIVATION.tagline}
          </h1>
          <Journeys journeys={MOTIVATION.journeys} />
          <div className="mt-8 flex flex-wrap justify-center gap-3">
            <Button asChild aria-keyshortcuts={me?.user ? "a" : "s"}>
              <Link href={me?.user ? "/account" : "/signup"}>
                {me?.user ? "Go to your account" : "Get started free"}
                <KeyLegend k={me?.user ? "a" : "s"} />
              </Link>
            </Button>
            <Button asChild variant="outline" aria-keyshortcuts="p">
              <Link href="/pricing">
                See pricing
                <KeyLegend k="p" />
              </Link>
            </Button>
          </div>
        </div>
      </section>
      <Section className="border-y bg-card">
        <div className="wrap grid items-center gap-10 md:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)]">
          <div>
            <h2 className="mb-4 text-[clamp(22px,3.4vw,28px)] leading-[1.2] tracking-[-0.02em]">{MOTIVATION.close}</h2>
            <p className="mb-4 max-w-[60ch] text-base text-muted-foreground">{MOTIVATION.body}</p>
            <p className="mb-4 text-base font-semibold">{MOTIVATION.ship}</p>
            <p className="max-w-[60ch] text-base text-muted-foreground">
              usecode is open source and freely distributed.{" "}
              <a href={REPO_URL} className="text-primary hover:underline">
                Have a look on GitHub
              </a>
              .
            </p>
          </div>
          <Activity lines={HERO_FEED} />
        </div>
      </Section>
      <Section>
        <div className="wrap grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {FEATURES.map((f) => (
            <Card key={f.title} className="gap-2 rounded-lg py-5 shadow-none">
              <CardContent className="px-5">
                <h3 className="mb-2">{f.title}</h3>
                <p className="text-[13px] text-muted-foreground">{f.body}</p>
              </CardContent>
            </Card>
          ))}
        </div>
      </Section>
      <Section className="border-y bg-card">
        <div className="wrap">
          <h2 className="mb-6 text-base">Up and running in three steps</h2>
          <ol className="grid gap-6 sm:grid-cols-3">
            {STEPS.map((s, i) => (
              <li key={s.title}>
                <span className="mb-3 inline-grid size-6 place-items-center rounded-full border border-border-strong font-mono text-xs text-muted-foreground">
                  {i + 1}
                </span>
                <h3 className="mb-1">{s.title}</h3>
                <p className="text-muted-foreground">{s.body}</p>
              </li>
            ))}
          </ol>
        </div>
      </Section>
      <Faqs />
    </>
  );
}
