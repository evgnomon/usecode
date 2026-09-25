// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Align CSV columns with padding so the file is readable in plain text editors.

mod align;
mod reader;
mod sniff;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use crate::sniff::Dialect;

/// Align CSV columns with padding so the file is readable in plain text editors.
#[derive(Parser)]
#[command(name = "tidycsv")]
struct Cli {
    /// input CSV file
    input: PathBuf,
    /// output file (default: stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// column separator in the aligned output (default: ' | ')
    #[arg(
        short,
        long,
        default_value = " | ",
        hide_default_value = true,
        allow_hyphen_values = true
    )]
    separator: String,
    /// extra spaces between columns (default: 2)
    #[arg(
        short,
        long,
        default_value_t = 2,
        hide_default_value = true,
        allow_negative_numbers = true
    )]
    padding: i64,
}

fn run(cli: &Cli) -> Result<(), String> {
    let bytes = std::fs::read(&cli.input).map_err(|e| format!("{}: {e}", cli.input.display()))?;
    let text = String::from_utf8(bytes).map_err(|e| format!("{}: {e}", cli.input.display()))?;
    let raw = text.strip_prefix('\u{feff}').unwrap_or(&text);

    let sample: String = raw.chars().take(8192).collect();
    let dialect = sniff::sniff(&sample, ",;\t|").unwrap_or(Dialect::EXCEL);

    let rows = reader::read(&reader::split_lines(raw), dialect)?;
    let Some(output) = align::align(&rows, &cli.separator, cli.padding) else {
        eprintln!("empty file");
        return Ok(());
    };

    match &cli.output {
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(output.as_bytes())
                .and_then(|()| out.flush())
                .map_err(|e| e.to_string())
        }
        Some(path) => std::fs::write(path, output).map_err(|e| format!("{}: {e}", path.display())),
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tidycsv: {e}");
            ExitCode::FAILURE
        }
    }
}
