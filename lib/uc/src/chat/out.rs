// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Where the chat's output goes: the terminal. `uc agent serve` runs this
//! same uc on a pseudo-terminal of its own, so a web page gets exactly what a
//! terminal would (see `serve`).

use std::io::{IsTerminal, Write};
use std::sync::{LazyLock, Mutex};

use super::hl::Highlighter;

/// The reply's highlighting, when stdout is a terminal and NO_COLOR is unset.
static HL: LazyLock<Option<Mutex<Highlighter>>> = LazyLock::new(|| {
    (std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
        .then(|| Mutex::new(Highlighter::default()))
});

/// Whether a spinner can turn: stderr is a terminal.
pub(super) fn tty() -> bool {
    std::io::stderr().is_terminal()
}

/// A status line: `· text` on the terminal, over the spinner when one is
/// turning (subagents note what they do while it does); it comes back on
/// the line after.
pub(super) fn note(text: &str) {
    if tty() {
        eprintln!("\r\x1b[2K· {text}");
    } else {
        eprintln!("· {text}");
    }
}

/// Text that is already laid out, like a list of models.
pub(super) fn raw(text: &str) {
    eprint!("{text}");
}

/// A piece of the model's reply as it streams in.
pub(super) fn text(text: &str) {
    match HL.as_ref() {
        Some(hl) => print!("{}", hl.lock().unwrap().push(text)),
        None => print!("{text}"),
    }
    let _ = std::io::stdout().flush();
}

/// What the reply's highlighting still holds back, printed: the reply ended
/// or was stopped.
pub(super) fn text_end() {
    if let Some(hl) = HL.as_ref() {
        print!("{}", hl.lock().unwrap().finish());
        let _ = std::io::stdout().flush();
    }
}

/// The model's reply is complete.
pub(super) fn reply_end() {
    text_end();
    println!("\n");
}

/// What a command prints, as it comes or as job_output reads it.
pub(super) fn output(_job: usize, bytes: &[u8]) {
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(bytes);
    let _ = err.flush();
}

/// A file about to be written or edited, from `before` to `after`.
pub(super) fn diff(_path: &str, before: &str, after: &str) {
    eprint!(
        "{}",
        super::diff::render(before, after, super::diff::color())
    );
}

/// The screen and its scrollback are cleared.
pub(super) fn clear() {
    print!("\x1b[H\x1b[2J\x1b[3J");
    let _ = std::io::stdout().flush();
}
