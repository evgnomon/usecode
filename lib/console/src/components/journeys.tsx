// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The hero's journeys, rolling like film credits, with the one in the middle
// played out in a console underneath: the prompt typed, then what the agent
// does about it, step by step, until it settles and the next one rolls in.

import { Check, LoaderCircle } from "lucide-react";
import { AnimatePresence, LazyMotion, MotionConfig, domAnimation, m as motion, useReducedMotion, type Transition } from "motion/react";
import { useEffect, useState } from "react";
import { Dot, Pill } from "@/components/console";
import type { Journey } from "@/content";
import { cn } from "@/lib/utils";

// The timeline of one journey, in ms: the row rolls in, the prompt types, each
// step takes its turn, the last one settles and the result holds for a read.
const ROLL_IN = 700;
const PER_CHAR = 32;
const THINK = 450;
const PER_STEP = 750;
const HOLD = 2600;

// Where a journey sits by its row from the middle. Classes for the HTML (the
// CSP blocks style attributes), motion targets once scripts take over.
const ROW_CLASS: Record<number, string> = {
  [-2]: "-translate-y-[200%] scale-90 opacity-0",
  [-1]: "-translate-y-full scale-[0.92] opacity-30 blur-[0.5px]",
  0: "translate-y-0 scale-100 opacity-100",
  1: "translate-y-full scale-[0.92] opacity-30 blur-[0.5px]",
  2: "translate-y-[200%] scale-90 opacity-0",
};
const ROW = (row: number) => ({
  y: `${row * 100}%`,
  scale: row === 0 ? 1 : Math.abs(row) === 1 ? 0.92 : 0.86,
  opacity: row === 0 ? 1 : Math.abs(row) === 1 ? 0.3 : 0,
  filter: row === 0 ? "blur(0px)" : `blur(${Math.abs(row) * 0.6}px)`,
});
const ROLL: Transition = { type: "spring", stiffness: 70, damping: 17, mass: 1 };

/** Rows from the middle: -1 is leaving, 1 is coming in, beyond that is off screen. */
function rowOf(i: number, at: number, n: number) {
  let row = (i - at + n) % n;
  if (row > n / 2) row -= n;
  return Math.max(-2, Math.min(2, row));
}

/** How far a journey has played: characters of the prompt typed, steps shown. */
type Play = { typed: number; shown: number; settled: boolean };
const DONE = (j: Journey): Play => ({ typed: j.prompt.length, shown: j.steps.length, settled: true });

/**
 * Plays journey `at` and calls `next` once it has settled and held. The first
 * one starts settled, as the HTML had it, so nothing flashes when scripts load.
 */
function usePlay(j: Journey, at: number, next: () => void, fresh: boolean): Play {
  const still = useReducedMotion();
  const [play, setPlay] = useState<Play>(() => (fresh ? DONE(j) : { typed: 0, shown: 0, settled: false }));

  useEffect(() => {
    const timers: ReturnType<typeof setTimeout>[] = [];
    const after = (ms: number, fn: () => void) => timers.push(setTimeout(fn, ms));
    let t = 0;
    if (fresh || still) {
      setPlay(DONE(j));
    } else {
      setPlay({ typed: 0, shown: 0, settled: false });
      t = ROLL_IN;
      for (let c = 1; c <= j.prompt.length; c++) {
        // A little unevenness, like someone typing; the same every time.
        t += PER_CHAR + ((c * 7) % 5) * 6;
        after(t, () => setPlay((p) => ({ ...p, typed: c })));
      }
      t += THINK;
      for (let s = 1; s <= j.steps.length; s++) {
        after(t, () => setPlay((p) => ({ ...p, shown: s })));
        t += PER_STEP;
      }
      after(t, () => setPlay((p) => ({ ...p, settled: true })));
    }
    // A hidden tab doesn't paint, so wait until it's seen again before rolling on.
    const go = () => (document.hidden ? document.addEventListener("visibilitychange", go, { once: true }) : next());
    after(t + HOLD, go);
    return () => {
      timers.forEach(clearTimeout);
      document.removeEventListener("visibilitychange", go);
    };
    // Only a new journey restarts the timeline.
  }, [at]);

  return play;
}

/** The "without" of the journey in the middle, word by word out of a blur. */
function Without({ text }: { text: string }) {
  return (
    <span className="text-primary">
      {text.split(" ").map((w, i) => (
        <motion.span
          key={`${w}${i}`}
          className="inline-block whitespace-pre"
          initial={{ opacity: 0, y: "0.35em", filter: "blur(6px)" }}
          animate={{ opacity: 1, y: 0, filter: "blur(0px)" }}
          transition={{ delay: 0.35 + i * 0.09, duration: 0.5, ease: [0.2, 0, 0, 1] }}
        >
          {i ? ` ${w}` : w}
        </motion.span>
      ))}
    </span>
  );
}

const ROWS_BOX = "relative h-[7.8em] overflow-hidden text-[clamp(19px,3vw,26px)] leading-[1.3] tracking-[-0.01em]";
const ROW_BOX = "absolute inset-x-0 top-[2.6em] flex h-[2.6em] items-center justify-center";

function Rows({ journeys, at, live, first }: { journeys: Journey[]; at: number; live: boolean; first: boolean }) {
  return (
    <div aria-hidden="true" className={ROWS_BOX}>
      {journeys.map((j, i) => {
        const row = rowOf(i, at, journeys.length);
        const words = (
          <span>
            {j.from} {live && !first && row === 0 ? <Without key={at} text={j.without} /> : <span className="text-primary">{j.without}</span>}
          </span>
        );
        return live ? (
          <motion.p key={j.from} className={ROW_BOX} initial={false} animate={ROW(row)} transition={ROLL}>
            {words}
          </motion.p>
        ) : (
          <p key={j.from} className={cn(ROW_BOX, ROW_CLASS[row])}>
            {words}
          </p>
        );
      })}
    </div>
  );
}

const STEP_IN = { initial: { opacity: 0, x: -8, filter: "blur(3px)" }, animate: { opacity: 1, x: 0, filter: "blur(0px)" } };

/** One journey as the agent's console shows it: your prompt, then its steps. */
function Console(props: { journey: Journey; play: Play; at: number; of: number; live: boolean; first: boolean }) {
  const { journey: j, play, at, of, live, first } = props;
  const working = !play.settled;
  const progress = (play.shown - (working && play.shown ? 0.5 : 0)) / j.steps.length;
  return (
    <div
      aria-hidden="true"
      className="mx-auto mt-6 max-w-[560px] overflow-hidden rounded-lg border bg-card text-left shadow-[0_1px_0_var(--border),0_12px_32px_-16px_var(--agent-glow)]"
    >
      <div className="flex items-center justify-between gap-3 border-b px-4 py-2 text-xs text-muted-foreground">
        <span className="inline-flex items-center gap-2">
          <Dot state={working ? "reconciling" : "settled"} className="size-1.5" />
          {working ? "Agent working" : "Agent idle"}
        </span>
        <span className="font-mono tabular-nums">
          {String(at + 1).padStart(2, "0")}/{of}
        </span>
      </div>
      <div className="h-[9.5rem] px-4 py-3 sm:h-[10rem] font-mono text-xs leading-normal sm:text-[13px]">
        <p className="mb-2 flex gap-2">
          <span className="text-primary select-none">›</span>
          <span>
            {j.prompt.slice(0, play.typed)}
            {play.typed < j.prompt.length || !play.shown ? (
              <span className="ml-px inline-block h-[1.1em] w-[0.55em] translate-y-[0.2em] animate-caret bg-primary/70" />
            ) : null}
          </span>
        </p>
        <ol>
          <AnimatePresence initial={false}>
            {j.steps.slice(0, play.shown).map((s, i) => {
              const busy = working && i === play.shown - 1;
              const Icon = busy ? LoaderCircle : Check;
              const icon = (
                <Icon className={cn("mt-0.5 size-3.5 shrink-0", busy ? "animate-spin text-primary" : "text-settled")} />
              );
              const text = <span className={cn(busy ? "text-foreground" : "text-muted-foreground")}>{s}</span>;
              return live ? (
                <motion.li key={`${at}${s}`} className="flex gap-2 py-[3px]"
                  initial={first ? false : STEP_IN.initial}
                  animate={STEP_IN.animate}
                  transition={{ duration: 0.3, ease: [0.2, 0, 0, 1] }}>
                  {icon}
                  {text}
                </motion.li>
              ) : (
                <li key={`${at}${s}`} className="flex gap-2 py-[3px]">
                  {icon}
                  {text}
                </li>
              );
            })}
          </AnimatePresence>
        </ol>
      </div>
      <div className="flex items-center justify-between gap-3 border-t px-4 py-2">
        <div className="h-0.5 flex-1 overflow-hidden rounded-full bg-border">
          {live ? (
            <motion.div
              className="h-full origin-left bg-primary"
              initial={false}
              animate={{ scaleX: progress }}
              transition={{ duration: play.shown ? 0.6 : 0.2, ease: [0.2, 0, 0, 1] }}
            />
          ) : (
            <div className="h-full bg-primary" />
          )}
        </div>
        <Pill state={working ? (play.shown ? "reconciling" : "desired") : "settled"}>
          {working ? (play.shown ? `${play.shown}/${j.steps.length}` : "Asked") : "Settled"}
        </Pill>
      </div>
    </div>
  );
}

export function Journeys({ journeys }: { journeys: Journey[] }) {
  // The HTML is built ahead of time without style attributes; motion only takes over once mounted.
  const [live, setLive] = useState(false);
  useEffect(() => setLive(true), []);
  const [at, setAt] = useState(0);
  const [fresh, setFresh] = useState(true);
  // The first journey starts settled, as the HTML had it.
  const first = fresh && at === 0;
  const journey = journeys[at] ?? journeys[0]!;
  const play = usePlay(journey, at, () => {
    setFresh(false);
    setAt((i) => (i + 1) % journeys.length);
  }, first);

  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
        {/* Screen readers get the whole list, not a moving target. */}
        <ul className="sr-only">
          {journeys.map((j) => (
            <li key={j.from}>
              {j.from} {j.without}
            </li>
          ))}
        </ul>
        <Rows journeys={journeys} at={at} live={live} first={first} />
        <Console journey={journey} play={play} at={at} of={journeys.length} live={live} first={first} />
      </MotionConfig>
    </LazyMotion>
  );
}
