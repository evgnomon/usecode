// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Activity, type Line } from "@/components/console";
import { Section } from "@/components/layout";
import { FAQS, FEATURES, MOTIVATION, REPO_URL, STEPS } from "@/content";
import { useMe } from "@/lib/api";
import { Link, useTitle } from "@/lib/router";

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
        <Accordion type="multiple">
          {FAQS.map((f) => (
            <AccordionItem key={f.q} value={f.q} className="last:border-b">
              <AccordionTrigger className="cursor-pointer">{f.q}</AccordionTrigger>
              <AccordionContent className="text-muted-foreground">{f.a}</AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>
      </div>
    </Section>
  );
}

export function Home() {
  useTitle("usecode.dev, container for agents");
  const { me } = useMe();
  return (
    <>
      <section className="pt-12 pb-8 sm:pt-16 sm:pb-12">
        <div className="wrap grid items-center gap-12 md:grid-cols-[minmax(0,1.15fr)_minmax(0,1fr)]">
          <div>
            <div className="mb-3 font-mono text-xs text-primary">{MOTIVATION.site}</div>
            <h1 className="mb-6 text-[clamp(26px,4.4vw,36px)] leading-[1.15] tracking-[-0.02em]">
              {MOTIVATION.tagline}
            </h1>
            {MOTIVATION.stanzas.map((lines) => (
              <ul key={lines[0]} className="mb-4 text-base text-muted-foreground">
                {lines.map((l) => (
                  <li key={l}>{l}</li>
                ))}
              </ul>
            ))}
            <p className="mb-6 text-base font-semibold">{MOTIVATION.close}</p>
            <p className="max-w-[60ch] text-base text-muted-foreground">
              When your coding agent can run code, it solves problems better and burns fewer tokens doing it, so it
              has more time left for the interesting stuff. To run code it needs a place to run it: an agent can't
              work in a void. usecode is that container.
            </p>
            <p className="mt-4 max-w-[60ch] text-base font-semibold">
              usecode is open source and freely distributed.{" "}
              <a href={REPO_URL} className="text-primary hover:underline">
                Have a look on GitHub
              </a>
              .
            </p>
            <div className="mt-6 flex flex-wrap gap-3">
              <Button asChild>
                <Link href={me?.user ? "/account" : "/signup"}>{me?.user ? "Go to your account" : "Get started free"}</Link>
              </Button>
              <Button asChild variant="outline">
                <Link href="/pricing">See pricing</Link>
              </Button>
            </div>
          </div>
          <Activity lines={HERO_FEED} />
        </div>
      </section>
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
