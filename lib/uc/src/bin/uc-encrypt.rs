// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-encrypt <file>` — encrypt a file to `<file>.asc` and remove the original
//! once the round trip has been verified.

use std::path::PathBuf;
use std::process::ExitCode;
use uc::cli;
use uc::crypto;
use uc::{Error, Result};

const SUMMARY: &str = "encrypt a file to <file>.asc and remove the original";

const USAGE: &str = "\
Usage: uc encrypt [-f] <file>

Encrypts <file> to <file>.asc and removes <file> once the encrypted copy has
been decrypted back and compared against it.

  -f  overwrite an existing <file>.asc
  -h  show this help";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Error::Usage(msg)) => {
            eprintln!("uc-encrypt: {msg}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(err) => {
            eprintln!("uc-encrypt: {err}");
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
    let opts = cli::parse(&args, "f")?;
    if opts.help {
        println!("{USAGE}");
        return Ok(());
    }

    let mut output = opts.file.clone().into_os_string();
    output.push(".asc");
    let output = PathBuf::from(output);
    cli::check_output(&output, opts.force, "use -f to overwrite")?;

    let plaintext = cli::read_file(&opts.file)?;

    let password = cli::prompt_password("enter encryption password: ")?;
    let verify = cli::prompt_password("verify encryption password: ")?;
    if *password != *verify {
        return Err(Error::Format("passwords do not match".to_string()));
    }
    if password.is_empty() {
        return Err(Error::Format("empty password".to_string()));
    }

    let raw = crypto::encrypt_raw(&plaintext, password.as_bytes())?;
    let armored = crypto::armor(&raw);

    // Verify the round trip before destroying the original.
    let decoded = crypto::dearmor(armored.as_bytes())?;
    let (check, _) = crypto::decrypt_raw(&decoded, password.as_bytes())?;
    if check != plaintext {
        return Err(Error::Format(
            "round-trip verification failed; original left untouched".to_string(),
        ));
    }

    cli::write_atomic(&output, armored.as_bytes(), 0o600)?;
    std::fs::remove_file(&opts.file)
        .map_err(|err| Error::Io(format!("removing '{}'", opts.file.display()), err))?;
    println!(
        "File encrypted as {} and original file removed",
        output.display()
    );
    Ok(())
}
