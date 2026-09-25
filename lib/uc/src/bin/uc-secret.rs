// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-secret` — generate secrets.

use clap::{Parser, Subcommand};
use std::process::ExitCode;
use uc::password::{self, Charset};

const SUMMARY: &str = "generate secrets";

/// Generate secrets.
#[derive(Parser)]
#[command(name = "uc secret", bin_name = "uc secret", version)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print a random password drawn from the OS random source.
    Gen {
        /// Length of the password.
        #[arg(short, long, default_value_t = 32)]
        length: usize,

        /// Leave letters out.
        #[arg(long)]
        no_letters: bool,

        /// Leave digits out.
        #[arg(long)]
        no_digits: bool,

        /// Leave symbols out.
        #[arg(long)]
        no_symbols: bool,
    },
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("{SUMMARY}");
        return ExitCode::SUCCESS;
    }
    let Cmd::Gen {
        length,
        no_letters,
        no_digits,
        no_symbols,
    } = Cli::parse().command;
    let charset = Charset {
        letters: !no_letters,
        digits: !no_digits,
        symbols: !no_symbols,
    };
    match password::generate(length, charset) {
        Ok(password) => {
            println!("{password}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("uc-secret: {err:#}");
            ExitCode::FAILURE
        }
    }
}
