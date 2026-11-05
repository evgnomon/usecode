// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

export interface Config {
  port: number;
  /** Where the console is reached, e.g. https://usecode.dev. */
  publicUrl: string;
  /** Where browsers reach the API, e.g. https://api.usecode.dev; the pages call it straight. */
  apiUrl: string;
  /** Where this server reaches the API, e.g. http://api:8000 inside the cluster; defaults to apiUrl. */
  apiInternalUrl: string;
}

const trim = (url: string) => url.replace(/\/+$/, "");

export function load(env: NodeJS.ProcessEnv = process.env): Config {
  const apiUrl = trim(env.API_URL || "http://localhost:8000");
  return {
    port: Number(env.PORT ?? 8080),
    publicUrl: trim(env.PUBLIC_URL ?? "http://localhost:8080"),
    apiUrl,
    apiInternalUrl: trim(env.API_INTERNAL_URL || apiUrl),
  };
}
