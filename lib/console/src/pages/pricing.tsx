// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { useState, type ReactNode } from "react";
import { Check } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { ErrorNote, Notice, PageHead, Section } from "@/components/layout";
import { CONTACT, PLANS, REPO_URL, TEAM_PRICE } from "@/content";
import { goTo, useMe } from "@/lib/api";
import { Link, useTitle } from "@/lib/router";
import { Faqs } from "./home";

type Plan = (typeof PLANS)[keyof typeof PLANS];

function PlanCard({ plan, period, children }: { plan: Plan; period?: string; children: ReactNode }) {
  return (
    <Card className="gap-0 rounded-lg py-6 shadow-none">
      <CardContent className="flex h-full flex-col px-6">
        <div className="font-mono text-xs text-primary">{plan.name}</div>
        <div className="my-2 text-[32px] font-medium tracking-[-0.02em]">
          {plan.price}{" "}
          {period ? <span className="text-sm font-normal tracking-normal text-muted-foreground">{period}</span> : null}
        </div>
        <p className="text-muted-foreground">{plan.note}</p>
        <ul className="mt-5 mb-6 grid gap-2">
          {plan.points.map((p) => (
            <li key={p} className="flex gap-2.5">
              <Check className="mt-0.5 size-4 shrink-0 text-settled" strokeWidth={1.5} />
              {p}
            </li>
          ))}
        </ul>
        <div className="mt-auto">{children}</div>
      </CardContent>
    </Card>
  );
}

function IndividualAction() {
  const { me } = useMe();
  return (
    <Button asChild variant="outline" className="w-full">
      {me?.user ? <Link href="/account">Go to your account</Link> : <Link href="/signup">Sign up free</Link>}
    </Button>
  );
}

function TeamAction() {
  const { me } = useMe();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const subscribe = async () => {
    setBusy(true);
    setError("");
    try {
      await goTo("/api/billing/checkout");
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };
  return (
    <>
      {error ? <ErrorNote>{error}</ErrorNote> : null}
      {!me ? null : me.subscribed ? (
        <Button asChild variant="outline" className="w-full">
          <Link href="/account">You're on Team, see your account</Link>
        </Button>
      ) : !me.user ? (
        <Button asChild className="w-full">
          <Link href="/signup?next=/pricing">Get started</Link>
        </Button>
      ) : me.billing ? (
        <Button className="w-full" disabled={busy} onClick={subscribe}>
          Start Team for {TEAM_PRICE.amount}/user a month
        </Button>
      ) : (
        <Notice>Team opens very soon. We'll let you know by email.</Notice>
      )}
    </>
  );
}

export function Pricing() {
  useTitle("Pricing · usecode.dev");
  return (
    <>
      <PageHead title="Free for you, simple for teams">
        Your own account is free, however many nodes you add. A team pays {TEAM_PRICE.amount} a month per person, never
        per node. For an enterprise setup, get in touch and we'll work it out together.
      </PageHead>
      <Section>
        <div className="wrap grid gap-4 md:grid-cols-3">
          <PlanCard plan={PLANS.individual}>
            <IndividualAction />
          </PlanCard>
          <PlanCard plan={PLANS.team} period={TEAM_PRICE.period}>
            <TeamAction />
          </PlanCard>
          <PlanCard plan={PLANS.enterprise}>
            <Button asChild variant="outline" className="w-full">
              <a href={CONTACT}>Contact us</a>
            </Button>
          </PlanCard>
        </div>
        <div className="wrap mt-10 max-w-[680px]">
          <h3 className="mb-3">Rather run it yourself?</h3>
          <p className="mb-3 font-semibold">usecode is open source and freely distributed.</p>
          <p className="text-muted-foreground">
            Grab it from{" "}
            <a href={REPO_URL} className="text-primary hover:underline">
              GitHub
            </a>
            . If you'd like help setting it up for your team, that's a paid engagement and we're happy to{" "}
            <a href={CONTACT} className="text-primary hover:underline">
              talk
            </a>
            .
          </p>
        </div>
      </Section>
      <Faqs />
    </>
  );
}
