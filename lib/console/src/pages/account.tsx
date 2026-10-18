// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { EmptyNodes, Pill } from "@/components/console";
import { KeyLegend } from "@/components/kbd";
import { ErrorNote, Notice, Section } from "@/components/layout";
import { PLANS, TEAM_PRICE } from "@/content";
import { goTo, useMe, type Me } from "@/lib/api";
import { Link, navigate, useLocation } from "@/lib/router";

const date = (iso: string) =>
  new Date(iso).toLocaleDateString("en-US", { year: "numeric", month: "long", day: "numeric", timeZone: "UTC" });

function Subscription({ me }: { me: Me }) {
  const [error, setError] = useState("");
  const sub = me.subscription;
  if (!me.subscribed || !sub) {
    return (
      <>
        <p className="mb-3 flex flex-wrap items-center gap-2">
          <Pill state="settled">Free</Pill>
          {PLANS.individual.name}
        </p>
        <p className="mb-3 text-muted-foreground">
          usecode.dev supervises all your nodes, edge or Kubernetes, for free. Working with others? Team is{" "}
          {TEAM_PRICE.amount} {TEAM_PRICE.period}.
        </p>
        <Button asChild variant="outline" aria-keyshortcuts="p">
          <Link href="/pricing">
            See the plans
            <KeyLegend k="p" />
          </Link>
        </Button>
      </>
    );
  }
  const pastDue = sub.status === "past_due";
  const manage = () => goTo("/api/billing/portal").catch((err: Error) => setError(err.message));
  return (
    <>
      <p className="mb-3 flex flex-wrap items-center gap-2">
        <Pill state={pastDue ? "drift" : "settled"} className="capitalize">
          {sub.status.replace("_", " ")}
        </Pill>
        {PLANS.team.name}, {TEAM_PRICE.amount} {TEAM_PRICE.period}
      </p>
      {pastDue ? <Notice>Your last payment didn't go through. Update your card to keep your team on.</Notice> : null}
      {sub.currentPeriodEnd ? (
        <p className="mb-3 text-muted-foreground">
          {sub.cancelAtPeriodEnd ? "Ends on" : "Renews on"} {date(sub.currentPeriodEnd)}
        </p>
      ) : null}
      {error ? <ErrorNote>{error}</ErrorNote> : null}
      {me.billing ? (
        <Button variant="outline" onClick={manage}>
          Manage billing
        </Button>
      ) : null}
    </>
  );
}

export function Account() {
  const { me, refresh } = useMe();
  const { query } = useLocation();
  const justSubscribed = query.get("subscribed") === "1";

  useEffect(() => {
    if (me && !me.user) navigate("/login?next=/account", { replace: true });
  }, [me]);
  // Back from Stripe, the webhook may land a moment after us: look again
  // every two seconds, for a minute.
  const [checks, setChecks] = useState(0);
  useEffect(() => {
    if (!justSubscribed || me?.subscribed || checks >= 30) return;
    const t = setTimeout(() => void refresh().then(() => setChecks((n) => n + 1)), 2000);
    return () => clearTimeout(t);
  }, [justSubscribed, me, refresh, checks]);

  if (!me?.user) return null;
  return (
    <Section>
      <div className="wrap max-w-[680px]">
        <h1 className="mb-3 text-xl">Your account</h1>
        <p className="mb-3 text-muted-foreground">Logged in as {me.user.email}</p>
        {justSubscribed && !me.subscribed ? (
          <Notice>Thanks! Your payment is being confirmed; this page catches up in a moment.</Notice>
        ) : null}
        <Card className="mb-8 gap-0 rounded-lg py-5 shadow-none">
          <CardContent className="px-5">
            <h3 className="mb-3">Plan</h3>
            <Subscription me={me} />
          </CardContent>
        </Card>
        <h2 className="mb-3 text-base">Nodes</h2>
        <EmptyNodes />
      </div>
    </Section>
  );
}
