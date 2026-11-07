// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { StrictMode } from "react";
import { createRoot, hydrateRoot } from "react-dom/client";
import { App } from "@/App";
import { MeProvider } from "@/lib/api";
import "@/index.css";

const root = document.getElementById("root")!;
const app = (
  <StrictMode>
    <MeProvider>
      <App />
    </MeProvider>
  </StrictMode>
);

// The content pages arrive already rendered (src/prerender.tsx); the rest start empty.
if (root.firstElementChild) hydrateRoot(root, app);
else createRoot(root).render(app);

// Makes usecode.dev installable as an app (public/sw.js); not in `npm run dev`.
if (import.meta.env.PROD && "serviceWorker" in navigator) {
  addEventListener("load", () => void navigator.serviceWorker.register("/sw.js"));
}
