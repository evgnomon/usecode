// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { useState, type FormEvent } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ErrorNote, Section } from "@/components/layout";
import { post, useMe } from "@/lib/api";
import { Link, navigate, useLocation, useTitle } from "@/lib/router";
import { safeNext } from "@/lib/safe-next";

export function AuthForm({ mode }: { mode: "login" | "signup" }) {
  const signup = mode === "signup";
  useTitle(`${signup ? "Sign up" : "Log in"} · usecode.dev`);
  const { refresh } = useMe();
  const { query } = useLocation();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await post(signup ? "/api/signup" : "/api/login", { email, password });
      await refresh();
      navigate(safeNext(query.get("next")), { replace: true });
    } catch (err) {
      setError((err as Error).message);
      setBusy(false);
    }
  };

  const next = query.get("next");
  const other = (path: string) => (next ? `${path}?next=${encodeURIComponent(next)}` : path);
  return (
    <Section>
      <div className="wrap max-w-[380px]">
        <h1 className="mb-3 text-xl">{signup ? "Make an account" : "Welcome back"}</h1>
        <p className="mb-5 text-muted-foreground">
          {signup ? "It takes a few seconds, and it's free." : "Log in to see your plan and nodes."}
        </p>
        {error ? <ErrorNote>{error}</ErrorNote> : null}
        <form onSubmit={submit} className="grid gap-3">
          <div className="grid gap-1.5">
            <Label htmlFor="email">Email</Label>
            <Input id="email" type="email" required autoComplete="email" value={email} onChange={(e) => setEmail(e.target.value)} />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="password">Password</Label>
            <Input
              id="password"
              type="password"
              required
              minLength={signup ? 10 : undefined}
              autoComplete={signup ? "new-password" : "current-password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            {signup ? <small className="text-xs text-muted-foreground">At least 10 characters.</small> : null}
          </div>
          <Button type="submit" className="w-full" disabled={busy}>
            {signup ? "Sign up" : "Log in"}
          </Button>
        </form>
        <p className="mt-5 text-muted-foreground">
          {signup ? "Already have an account? " : "New here? "}
          <Link href={other(signup ? "/login" : "/signup")} className="text-primary hover:underline">
            {signup ? "Log in" : "Make an account"}
          </Link>
        </p>
      </div>
    </Section>
  );
}
