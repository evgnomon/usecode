// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! CLI: read JSONC on stdin, write JSON on stdout.
//!
//! Use:  jsonc < input.jsonc > out.json

use std::io::{Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut src = Vec::new();
    if let Err(err) = std::io::stdin().lock().read_to_end(&mut src) {
        eprintln!("error reading input: {err}");
        return ExitCode::FAILURE;
    }

    // Convert in place — saves an allocation.
    let n = jsonc::to_json_in_place(&mut src);

    let mut stdout = std::io::stdout().lock();
    if let Err(err) = stdout.write_all(&src[..n]).and_then(|()| stdout.flush()) {
        eprintln!("error writing output: {err}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
