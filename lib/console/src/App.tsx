// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import type { ReactNode } from "react";
import { Layout } from "@/components/layout";
import { useLocation } from "@/lib/router";
import { Account } from "@/pages/account";
import { AuthForm } from "@/pages/auth";
import { Design } from "@/pages/design";
import { Home } from "@/pages/home";
import { NotFound } from "@/pages/not-found";
import { Pricing } from "@/pages/pricing";

// Keep in step with PAGES in server/app.ts.
const PAGES: Record<string, () => ReactNode> = {
  "/": () => <Home />,
  "/pricing": () => <Pricing />,
  "/login": () => <AuthForm key="login" mode="login" />,
  "/signup": () => <AuthForm key="signup" mode="signup" />,
  "/account": () => <Account />,
  "/design": () => <Design />,
};

export function App() {
  const { path } = useLocation();
  return <Layout>{(PAGES[path] ?? (() => <NotFound />))()}</Layout>;
}
