// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { useState, type FormEvent, type MouseEvent } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ErrorNote, Section } from "@/components/layout";
import { post, useMe, type ApiError } from "@/lib/api";
import { Link, navigate, useLocation } from "@/lib/router";
import { safeNext } from "@/lib/safe-next";

const TEXT = {
  signup: {
    title: "Make an account",
    lead: "It takes a few seconds, and it's free. We'll mail you a code to check it's you.",
    api: "/api/signup",
    submit: "Sign up",
    finish: "Make my account",
  },
  login: {
    title: "Welcome back",
    lead: "Log in to see your plan and nodes. We'll mail you a code to check it's you.",
    api: "/api/login",
    submit: "Log in",
    finish: "Log in",
  },
  reset: {
    title: "Forgot your password?",
    lead: "Pick a new one, and we'll mail you a code to check it's you. Then you're logged in with it.",
    api: "/api/reset",
    submit: "Send me a code",
    finish: "Set my new password",
  },
} as const;

export function AuthForm({ mode }: { mode: "login" | "signup" | "reset" }) {
  const text = TEXT[mode];
  // A new password is picked on sign up and on a reset.
  const fresh = mode !== "login";
  const { refresh } = useMe();
  const { query } = useLocation();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  // The address the code went to, once it's sent.
  const [sentTo, setSentTo] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent | MouseEvent) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const res = await post<{ email: string }>(text.api, { email, password });
      setCode("");
      setSentTo(res.email);
    } catch (err) {
      setError((err as Error).message);
    }
    setBusy(false);
  };

  const verify = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await post("/api/verify", { code });
      await refresh();
      navigate(safeNext(query.get("next")), { replace: true });
    } catch (err) {
      // A code that ran out takes them back to the password, for a new one.
      if ((err as ApiError).status === 410) setSentTo("");
      setError((err as Error).message);
      setBusy(false);
    }
  };

  const next = query.get("next");
  const other = (path: string) => (next ? `${path}?next=${encodeURIComponent(next)}` : path);
  return (
    <Section>
      <div className="wrap max-w-[380px]">
        <h1 className="mb-3 text-xl">{text.title}</h1>
        <p className="mb-5 text-muted-foreground">{text.lead}</p>
        {error ? <ErrorNote>{error}</ErrorNote> : null}
        {sentTo ? (
          <form onSubmit={verify} className="grid gap-3">
            <p className="text-muted-foreground">
              {mode === "reset" ? "If there's an account for " : "We sent a code to "}
              <span className="text-foreground">{sentTo}</span>
              {mode === "reset" ? ", we've mailed it a code." : "."} It works for 10 minutes.
            </p>
            <div className="grid gap-1.5">
              <Label htmlFor="code">Code</Label>
              <Input
                id="code"
                required
                autoFocus
                inputMode="numeric"
                autoComplete="one-time-code"
                maxLength={9}
                value={code}
                onChange={(e) => setCode(e.target.value)}
              />
            </div>
            <Button type="submit" className="w-full" disabled={busy}>
              {text.finish}
            </Button>
            <Button type="button" variant="ghost" className="w-full" disabled={busy} onClick={submit}>
              Send a new code
            </Button>
          </form>
        ) : (
          <form onSubmit={submit} className="grid gap-3">
            <div className="grid gap-1.5">
              <Label htmlFor="email">Email</Label>
              <Input id="email" type="email" required autoComplete="email" value={email} onChange={(e) => setEmail(e.target.value)} />
            </div>
            <div className="grid gap-1.5">
              <div className="flex items-baseline justify-between">
                <Label htmlFor="password">{mode === "reset" ? "New password" : "Password"}</Label>
                {mode === "login" ? (
                  <Link href={other("/forgot")} className="text-xs text-primary hover:underline">
                    Forgot it?
                  </Link>
                ) : null}
              </div>
              <Input
                id="password"
                type="password"
                required
                minLength={fresh ? 10 : undefined}
                autoComplete={fresh ? "new-password" : "current-password"}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
              {fresh ? <small className="text-xs text-muted-foreground">At least 10 characters.</small> : null}
            </div>
            <Button type="submit" className="w-full" disabled={busy}>
              {text.submit}
            </Button>
          </form>
        )}
        <p className="mt-5 text-muted-foreground">
          {mode === "login" ? "New here? " : mode === "signup" ? "Already have an account? " : "Remembered it? "}
          <Link href={other(mode === "login" ? "/signup" : "/login")} className="text-primary hover:underline">
            {mode === "login" ? "Make an account" : "Log in"}
          </Link>
        </p>
      </div>
    </Section>
  );
}
