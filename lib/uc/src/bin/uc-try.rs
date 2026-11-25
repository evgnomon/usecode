// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-try`: `uc try`, see [`uc::tries`].

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--summary") {
        println!("dated scratch directories, a fuzzy pick away (the `try` shell function)");
        return ExitCode::SUCCESS;
    }
    uc::tries::main(&args)
}
