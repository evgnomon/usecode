// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// `npm run dev` serves the pages here and sends the API to `make serve`.
const api = process.env.API_URL ?? "http://localhost:8080";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  build: { outDir: "dist/client", emptyOutDir: true },
  server: { proxy: { "/api": api, "/stripe": api, "/healthz": api } },
});
