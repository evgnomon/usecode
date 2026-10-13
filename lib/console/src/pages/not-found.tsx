// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

import { Section } from "@/components/layout";
import { Link } from "@/lib/router";

export function NotFound() {
  return (
    <Section>
      <div className="wrap max-w-[680px]">
        <h1 className="mb-3 text-xl">Nothing here</h1>
        <p className="text-muted-foreground">
          That page doesn't exist.{" "}
          <Link href="/" className="text-primary hover:underline">
            Back home
          </Link>
        </p>
      </div>
    </Section>
  );
}
