// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { useEffect, useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Pill } from "@/components/console";
import { KeyLegend } from "@/components/kbd";
import { ErrorNote, Notice, Section } from "@/components/layout";
import { HOSTED_PRICE, PLANS } from "@/content";
import { apiUrl, goTo, post, useMe, type Me } from "@/lib/api";
import { safeNext } from "@/lib/safe-next";
import { UsernameField } from "@/pages/auth";
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
          Every feature is yours, on all your nodes, for free. Want us to host the monitoring, audit log and backups?
          Hosted is {HOSTED_PRICE.amount} {HOSTED_PRICE.period}.
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
  const manage = () => goTo("/billing/portal").catch((err: Error) => setError(err.message));
  return (
    <>
      <p className="mb-3 flex flex-wrap items-center gap-2">
        <Pill state={pastDue ? "drift" : "settled"} className="capitalize">
          {sub.status.replace("_", " ")}
        </Pill>
        {PLANS.hosted.name}, {HOSTED_PRICE.amount} {HOSTED_PRICE.period}
      </p>
      {pastDue ? <Notice>Your last payment didn't go through. Update your card to keep Hosted on.</Notice> : null}
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

/**
 * For an account from before usernames: it picks one, once. Sent here by a
 * login for another site, it goes on with it after (`next`).
 */
function PickUsername({ refresh, next }: { refresh: () => Promise<void>; next: string | null }) {
  const [username, setUsername] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const pick = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await post("/account/username", { username });
      await refresh();
      if (next) navigate(safeNext(next, "/account", apiUrl()), { replace: true });
    } catch (err) {
      setError((err as Error).message);
    }
    setBusy(false);
  };
  return (
    <Card className="mb-8 gap-0 rounded-lg py-5 shadow-none">
      <CardContent className="px-5">
        <h3 className="mb-3">Pick a username</h3>
        <p className="mb-3 text-muted-foreground">
          Your account doesn't have one yet. You'll need it to log in with usecode.dev elsewhere, like our git.
        </p>
        {error ? <ErrorNote>{error}</ErrorNote> : null}
        <form onSubmit={pick} className="grid max-w-[380px] gap-3">
          <UsernameField value={username} onChange={setUsername} />
          <Button type="submit" variant="outline" disabled={busy}>
            Keep this username
          </Button>
        </form>
      </CardContent>
    </Card>
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
        <p className="mb-3 text-muted-foreground">
          Logged in as{" "}
          {me.user.username ? (
            <>
              <span className="text-foreground">{me.user.username}</span> ({me.user.email})
            </>
          ) : (
            me.user.email
          )}
        </p>
        {justSubscribed && !me.subscribed ? (
          <Notice>Thanks! Your payment is being confirmed; this page catches up in a moment.</Notice>
        ) : null}
        {me.user.username ? null : <PickUsername refresh={refresh} next={query.get("next")} />}
        <Card className="mb-8 gap-0 rounded-lg py-5 shadow-none">
          <CardContent className="px-5">
            <h3 className="mb-3">Plan</h3>
            <Subscription me={me} />
          </CardContent>
        </Card>
        <h2 className="mb-3 text-base">Hosts</h2>
        <p className="mb-3 text-muted-foreground">
          Every host running the usecode daemon can check in here, so you see which are up and how they're doing.
        </p>
        <Button asChild variant="outline" aria-keyshortcuts="o">
          <Link href="/hosts">
            Your hosts
            <KeyLegend k="o" />
          </Link>
        </Button>
      </div>
    </Section>
  );
}
