// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-decrypt <file>.asc` — decrypt back to `<file>`, keeping the `.asc`.

use std::io::Write;
use std::process::ExitCode;
use uc::cli;
use uc::crypto::{self, Kdf};
use uc::{Error, Result};

const SUMMARY: &str = "decrypt a <file>.asc back to <file>, keeping the encrypted file";

const USAGE: &str = "\
Usage: uc decrypt [-c] [-f] <file>.asc

Decrypts <file>.asc to <file>, keeping the encrypted file.

  -c  write the plaintext to stdout instead of a file
  -f  overwrite an existing <file>
  -h  show this help";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Usage(msg)) => {
            eprintln!("uc-decrypt: {msg}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(err) => {
            eprintln!("uc-decrypt: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--summary") {
        println!("{SUMMARY}");
        return Ok(());
    }
    let opts = cli::parse(&args, "cf")?;
    if opts.help {
        println!("{USAGE}");
        return Ok(());
    }

    let plain = match opts.file.extension() {
        Some(ext) if ext == "asc" => opts.file.with_extension(""),
        _ => {
            return Err(Error::Format(format!(
                "'{}' does not end in .asc",
                opts.file.display()
            )));
        }
    };
    if !opts.to_stdout {
        cli::check_output(&plain, opts.force, "use -f to overwrite, -c for stdout")?;
    }

    let armored = cli::read_file(&opts.file)?;
    let raw = crypto::dearmor(&armored)
        .map_err(|err| Error::Format(format!("'{}' {err}", opts.file.display())))?;

    let password = cli::prompt_password("enter decryption password: ")?;
    let (plaintext, kdf) = crypto::decrypt_raw(&raw, password.as_bytes())?;
    if kdf == Kdf::LegacyMd5 {
        eprintln!("Note: decrypted with the legacy (MD5) key derivation; re-encrypt to upgrade.");
    }

    if opts.to_stdout {
        std::io::stdout()
            .write_all(&plaintext)
            .map_err(|err| Error::Io("writing to stdout".to_string(), err))?;
    } else {
        cli::write_atomic(&plain, &plaintext, 0o600)?;
        eprintln!("Decrypted to {}", plain.display());
    }
    Ok(())
}
