// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// A key the console listens for, drawn as a little keycap. On a button it
// sits right after the label as a small thin box, like the legend on the
// front edge of a keycap; in a list it sits inline at full size.
// `aria-keyshortcuts` on the button says it to anything reading the page;
// the letter itself is decoration.

import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <kbd
      aria-hidden="true"
      className={cn(
        "inline-flex h-3.5 min-w-3.5 items-center justify-center border border-border-strong/70 bg-card px-1 font-sans text-[9px] leading-none font-normal text-muted-foreground",
        className,
      )}
    >
      {children}
    </kbd>
  );
}

/** The key a button answers to, right after its label in a small thin box. */
export function KeyLegend({ k, className }: { k: string; className?: string }) {
  return (
    <Kbd
      className={cn(
        "-my-1 h-3 min-w-3 px-0.5 text-[8px]",
        className,
      )}
    >
      {k}
    </Kbd>
  );
}