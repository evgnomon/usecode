// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// A key the console listens for, drawn as a little keycap. On a button it
// sits on the bottom face, like the legend on the front edge of a keycap;
// in a list it sits inline. `aria-keyshortcuts` on the button says it to
// anything reading the page; the letter itself is decoration.

import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function Kbd({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <kbd
      aria-hidden="true"
      className={cn(
        "inline-flex h-3.5 min-w-3.5 items-center justify-center rounded-[3px] border border-border-strong/70 bg-card px-1 font-sans text-[9px] leading-none font-normal text-muted-foreground",
        className,
      )}
    >
      {children}
    </kbd>
  );
}

/** The key a button answers to, on the button's bottom face. */
export function KeyLegend({ k, className }: { k: string; className?: string }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "pointer-events-none absolute inset-x-0 bottom-0 flex h-[3px] items-end justify-center pb-px font-sans text-[9px] leading-none text-muted-foreground",
        className,
      )}
    >
      {k}
    </span>
  );
}