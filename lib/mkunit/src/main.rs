// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! mkunit - Create a systemd unit file to run a command.

mod argv;
mod unit;

use std::ffi::OsString;
use std::io::ErrorKind;
use std::process::{Command, ExitCode};

use clap::Parser;

const USAGE: &str = "mkunit <unit-name> <command> [args...] [options]";

/// Create a systemd unit file to run a command.
///
/// Options must come before the unit name; everything after the unit name
/// is the command to run.
#[derive(Parser)]
#[command(name = "mkunit", override_usage = USAGE, infer_long_args = true)]
struct Cli {
    /// Name of the systemd unit (without .service)
    #[arg(allow_negative_numbers = true)]
    unit_name: String,
    /// Command to run
    #[arg(num_args = 0.., value_name = "COMMAND")]
    command: Vec<String>,
    /// Unit description (defaults to 'Run: <command>')
    #[arg(short, long)]
    description: Option<String>,
    /// Output path (default: <unit-name>.service in current dir)
    #[arg(short, long)]
    output: Option<String>,
    /// Install to /etc/systemd/system/ (requires root)
    #[arg(short, long)]
    install: bool,
    /// Enable the unit after installing (implies --install)
    #[arg(short, long)]
    enable: bool,
}

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl")
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

fn main() -> ExitCode {
    let mut raw = std::env::args_os();
    let prog = raw.next().unwrap_or_else(|| OsString::from("mkunit"));
    let args: Vec<String> = raw.map(|a| a.to_string_lossy().into_owned()).collect();
    let split = argv::split(&args);
    let cli =
        Cli::parse_from(std::iter::once(prog.to_string_lossy().into_owned()).chain(split.head));

    if split.command.is_empty() {
        eprintln!("usage: {USAGE}\nmkunit: error: You must provide a command to run.");
        return ExitCode::from(2);
    }

    let exec_start = split.command.join(" ");
    let description = match &cli.description {
        Some(d) if !d.is_empty() => d.clone(),
        _ => format!("Run: {exec_start}"),
    };
    let unit_name_full = unit::full_name(&cli.unit_name);
    let content = unit::render(&description, &exec_start);

    let install = cli.install || cli.enable;
    let output_path = if install {
        format!("/etc/systemd/system/{unit_name_full}")
    } else {
        unit::display_path(cli.output.as_deref().unwrap_or(&unit_name_full))
    };

    if let Err(e) = std::fs::write(&output_path, content) {
        if e.kind() == ErrorKind::PermissionDenied {
            eprintln!("Error: Permission denied writing to {output_path}");
            eprintln!("Tip: Run with sudo, or omit --install to write locally.");
        } else {
            eprintln!("Error: writing to {output_path}: {e}");
        }
        return ExitCode::FAILURE;
    }

    println!("✓ Created: {output_path}");
    println!("  Command: {exec_start}");

    if install {
        println!("\nReloading systemd daemon...");
        if systemctl(&["daemon-reload"]) {
            println!("✓ Daemon reloaded.");
        } else {
            eprintln!("Warning: daemon-reload failed.");
        }

        if cli.enable {
            println!("Enabling {unit_name_full}...");
            if systemctl(&["enable", &unit_name_full]) {
                println!("✓ Enabled {unit_name_full}.");
            } else {
                eprintln!("Warning: Failed to enable {unit_name_full}.");
            }
        }
    } else {
        println!("\nTo install and start:");
        println!("  sudo cp {output_path} /etc/systemd/system/");
        println!("  sudo systemctl daemon-reload");
        println!("  sudo systemctl enable --now {unit_name_full}");
    }

    ExitCode::SUCCESS
}
