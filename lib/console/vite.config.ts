// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// `npm run dev` serves the pages here; they call the API straight, on
// http://localhost:8000 (`make -C lib/api run`).
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  // The terminal (`restty`, ~3 MB) is already its own chunk, loaded with
  // `import()` only when a workspace opens, so the limit sits above it.
  build: { outDir: "dist/client", emptyOutDir: true, chunkSizeWarningLimit: 3200 },
});
