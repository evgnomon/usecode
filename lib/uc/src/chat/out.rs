// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Where the chat's output goes: the terminal. `uc agent serve` runs this
//! same uc on a pseudo-terminal of its own, so a web page gets exactly what a
//! terminal would (see `serve`).

use std::io::{IsTerminal, Write};

/// Whether a spinner can turn: stderr is a terminal.
pub(super) fn tty() -> bool {
    std::io::stderr().is_terminal()
}

/// A status line: `· text` on the terminal.
pub(super) fn note(text: &str) {
    eprintln!("· {text}");
}

/// Text that is already laid out, like a list of models.
pub(super) fn raw(text: &str) {
    eprint!("{text}");
}

/// A piece of the model's reply as it streams in.
pub(super) fn text(text: &str) {
    print!("{text}");
    let _ = std::io::stdout().flush();
}

/// The model's reply is complete.
pub(super) fn reply_end() {
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
