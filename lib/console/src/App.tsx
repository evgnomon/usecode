// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import type { ReactNode } from "react";
import { Layout } from "@/components/layout";
import { useLocation, useTitle } from "@/lib/router";
import { Account } from "@/pages/account";
import { AuthForm } from "@/pages/auth";
import { Home } from "@/pages/home";
import { NotFound } from "@/pages/not-found";
import { Pricing } from "@/pages/pricing";

export interface Page {
  title: string;
  render: () => ReactNode;
  /** Built into HTML ahead of time (src/prerender.tsx), so it reads without JavaScript. */
  prerender?: boolean;
}

// Keep in step with PAGES in server/app.ts.
export const PAGES: Record<string, Page> = {
  "/": { title: "usecode.dev, container for agents", render: () => <Home />, prerender: true },
  "/pricing": { title: "Pricing · usecode.dev", render: () => <Pricing />, prerender: true },
  "/login": { title: "Log in · usecode.dev", render: () => <AuthForm key="login" mode="login" /> },
  "/signup": { title: "Sign up · usecode.dev", render: () => <AuthForm key="signup" mode="signup" /> },
  "/forgot": { title: "Forgot your password · usecode.dev", render: () => <AuthForm key="reset" mode="reset" /> },
  "/account": { title: "Your account · usecode.dev", render: () => <Account /> },
};

export const NOT_FOUND: Page = { title: "Not found · usecode.dev", render: () => <NotFound />, prerender: true };

export function App() {
  const { path } = useLocation();
  const page = PAGES[path] ?? NOT_FOUND;
  useTitle(page.title);
  return <Layout>{page.render()}</Layout>;
}
