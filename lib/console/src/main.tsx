// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "@/App";
import { MeProvider } from "@/lib/api";
import "@/index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MeProvider>
      <App />
    </MeProvider>
  </StrictMode>,
);
