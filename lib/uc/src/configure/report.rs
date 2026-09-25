// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Progress output. Tasks finish in any order, so every line names its task
//! and is written whole, never interleaved with another task's line.

use crate::configure::engine::{Outcome, Summary};
use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const MAGENTA: &str = "\x1b[35m";

/// A task running this long gets a start line.
const SLOW_TASK: Duration = Duration::from_secs(2);

/// How much a failed command's output is echoed without `--verbose`.
const FAILURE_LINES: usize = 40;

pub struct Reporter {
    color: bool,
    verbose: bool,
    silent: bool,
    width: usize,
    total: usize,
    done: Mutex<usize>,
}

impl Reporter {
    pub fn new(color: bool, verbose: bool, width: usize, total: usize) -> Reporter {
        Reporter {
            color,
            verbose,
            silent: false,
            width: width.min(44),
            total,
            done: Mutex::new(0),
        }
    }

    pub fn silent() -> Reporter {
        Reporter {
            silent: true,
            ..Reporter::new(false, false, 0, 0)
        }
    }

    fn paint(&self, color: &str, text: &str) -> String {
        if self.color {
            format!("{color}{text}{RESET}")
        } else {
            text.to_string()
        }
    }

    fn emit(&self, text: String) {
        if self.silent {
            return;
        }
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "{text}");
    }

    fn progress(&self) -> String {
        let mut done = self.done.lock().unwrap_or_else(|p| p.into_inner());
        *done += 1;
        let digits = self.total.to_string().len();
        self.paint(DIM, &format!("[{:>digits$}/{}]", *done, self.total))
    }

    fn line(&self, badge: String, id: &str, rest: &str) {
        let id = format!("{id:<width$}", width = self.width);
        self.emit(format!("{badge} {id} {rest}").trim_end().to_string());
    }

    /// How long a task runs before its start is shown: at once with
    /// `--verbose`, so the command output that follows has a heading.
    pub fn start_delay(&self) -> Duration {
        if self.verbose {
            Duration::ZERO
        } else {
            SLOW_TASK
        }
    }

    pub fn started(&self, id: &str, name: &str) {
        let pad = " ".repeat(self.total.to_string().len() * 2 + 3);
        let badge = format!("{pad}{}", self.paint(CYAN, "  start"));
        self.line(badge, id, &self.paint(DIM, name));
    }

    pub fn finished(&self, id: &str, outcome: &Outcome, took: Duration) {
        let (word, color, detail) = match outcome {
            Outcome::Ok => ("     ok", GREEN, String::new()),
            Outcome::Changed => ("changed", YELLOW, String::new()),
            Outcome::Skipped(why) => ("   skip", CYAN, why.clone()),
        };
        let badge = format!("{} {}", self.progress(), self.paint(color, word));
        let took = self.paint(DIM, &elapsed(took));
        self.line(badge, id, &format!("{took} {detail}"));
    }

    pub fn failed(&self, id: &str, msg: &str, took: Duration, ignored: bool) {
        let (word, color) = if ignored {
            ("ignored", MAGENTA)
        } else {
            (" FAILED", RED)
        };
        let badge = format!("{} {}", self.progress(), self.paint(color, word));
        self.line(badge, id, &self.paint(DIM, &elapsed(took)));
        let lines: Vec<&str> = msg.lines().collect();
        let skip = lines.len().saturating_sub(FAILURE_LINES);
        if skip > 0 && !self.verbose {
            self.emit(format!("        │ … {skip} earlier lines"));
        }
        let shown = if self.verbose {
            &lines[..]
        } else {
            &lines[skip..]
        };
        for line in shown {
            self.emit(format!("        {} {line}", self.paint(color, "│")));
        }
    }

    pub fn skipped(&self, id: &str, reason: &str) {
        let badge = format!("{} {}", self.progress(), self.paint(CYAN, "   skip"));
        self.line(badge, id, &self.paint(DIM, reason));
    }

    pub fn blocked(&self, id: &str, reason: &str) {
        let badge = format!("{} {}", self.progress(), self.paint(RED, "blocked"));
        self.line(badge, id, &self.paint(DIM, reason));
    }

    pub fn log(&self, id: &str, line: &str) {
        if self.verbose {
            self.emit(format!("{} {line}", self.paint(DIM, &format!("{id} │"))));
        }
    }

    pub fn note(&self, id: &str, msg: &str) {
        self.emit(format!("{} {msg}", self.paint(BOLD, &format!("{id}:"))));
    }

    pub fn recap(&self, s: &Summary) {
        let mut parts = vec![
            self.paint(GREEN, &format!("ok={}", s.ok)),
            self.paint(YELLOW, &format!("changed={}", s.changed)),
            self.paint(CYAN, &format!("skipped={}", s.skipped)),
        ];
        if s.ignored > 0 {
            parts.push(self.paint(MAGENTA, &format!("ignored={}", s.ignored)));
        }
        let red = if s.failed > 0 || s.blocked > 0 {
            RED
        } else {
            DIM
        };
        parts.push(self.paint(red, &format!("failed={}", s.failed)));
        parts.push(self.paint(red, &format!("blocked={}", s.blocked)));
        self.emit(String::new());
        self.emit(format!(
            "{} {}  {}",
            self.paint(BOLD, "recap"),
            parts.join(" "),
            self.paint(DIM, &format!("in {}", elapsed(s.elapsed)))
        ));
    }
}

fn elapsed(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        format!("{}m{:02}s", d.as_secs() / 60, d.as_secs() % 60)
    }
}
