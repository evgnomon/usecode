// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::process::ExitCode;

use ::vm::config::Config;
use ::vm::error::Result;
use ::vm::libvirt::Connection;
use ::vm::vm::{self, MountSpec, VmSpecs};
use ::vm::{VERSION, error, warn};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let prog_name = args.first().map(String::as_str).unwrap_or("vm");

    let Some(command) = args.get(1).map(String::as_str) else {
        print_usage(prog_name);
        std::process::exit(1);
    };

    // Handle help and version flags
    match command {
        "--help" | "-h" => {
            print_help();
            return Ok(());
        }
        "--version" | "-v" => {
            print_version();
            return Ok(());
        }
        _ => {}
    }

    // Load configuration
    let cfg = match Config::load_from_file("/etc/vm/config.yaml") {
        Ok(cfg) => cfg,
        Err(::vm::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => Config::new(),
        Err(err) => {
            warn!("Could not load config: {err}");
            Config::new()
        }
    };

    // Open libvirt connection
    let conn = Connection::open("qemu:///system")?;

    // Ensure default network is active
    conn.ensure_default_network()?;

    let rest = &args[2..];

    match command {
        "create" => cmd_create(rest, &conn, &cfg),
        "list" => {
            let all = rest.iter().any(|arg| arg == "-a" || arg == "--all");
            vm::list_vms(&conn, all)
        }
        "info" => vm::show_vm_info(&conn, domain_arg(rest, "vm info <name>")),
        "inspect" => vm::inspect_vm(&conn, domain_arg(rest, "vm inspect <name>")),
        "start" => vm::start_vm(&conn, &cfg, domain_arg(rest, "vm start <name>")),
        "stop" => {
            let name = domain_arg(rest, "vm stop <name> [--force]");
            let force = parse_force(&rest[1..]);
            vm::stop_vm(&conn, name, force)
        }
        "restart" => {
            let name = domain_arg(rest, "vm restart <name> [--force]");
            let force = parse_force(&rest[1..]);
            vm::restart_vm(&conn, &cfg, name, force)
        }
        "delete" => {
            let name = domain_arg(rest, "vm delete <name> [--force]");
            let force = parse_force(&rest[1..]);
            vm::delete_vm(&conn, &cfg, name, force)
        }
        "ip" => vm::get_vm_ip(&conn, &cfg, domain_arg(rest, "vm ip <name>")),
        "snapshot" => cmd_snapshot(rest, &conn),
        "fork" => cmd_fork(rest, &conn, &cfg),
        "mount" => cmd_mount(rest, &conn),
        "config" => cmd_config(rest, &conn),
        // Legacy mode: treat as create command
        _ => cmd_create(&args[1..], &conn, &cfg),
    }
}

/// Prints `usage` and exits when no domain name was given.
fn domain_arg<'a>(args: &'a [String], usage: &str) -> &'a str {
    match args.first() {
        Some(name) => name.as_str(),
        None => fail_usage("Error: domain name required", usage),
    }
}

fn fail_usage(message: &str, usage: &str) -> ! {
    error!("{message}");
    error!("Usage: {usage}");
    std::process::exit(1);
}

fn parse_force(args: &[String]) -> bool {
    let mut force = false;
    for arg in args {
        if arg == "--force" {
            force = true;
        } else {
            error!("Error: unknown option: {arg}");
            std::process::exit(1);
        }
    }
    force
}

/// Reads the value that follows `args[i]`, exiting when it is missing.
fn option_value<'a>(args: &'a [String], i: usize, flag: &str) -> &'a str {
    match args.get(i + 1) {
        Some(value) => value.as_str(),
        None => {
            error!("Error: {flag} requires a value");
            std::process::exit(1);
        }
    }
}

fn parse_or_exit<T, E: std::fmt::Display>(result: std::result::Result<T, E>, what: &str) -> T {
    match result {
        Ok(value) => value,
        Err(err) => {
            error!("Error: invalid {what} value: {err}");
            std::process::exit(1);
        }
    }
}

fn print_usage(prog_name: &str) {
    println!("Usage: {prog_name} <command> [options]");
    println!("   or: {prog_name} <domain-name>  (legacy mode)\n");
    println!("Run '{prog_name} --help' for more information.");
}

fn print_help() {
    print!(
        r#"
vm - A lightweight KVM/QEMU virtual machine creation tool

Usage:
  vm <command> [options]
  vm <domain-name>                    (legacy mode: create VM)

Commands:
  create <name> [options]            Create a new VM
  list [-a|--all]                    List running VMs (or all with -a)
  info <name>                        Show VM information
  inspect <name>                     Show detailed VM info (RAM, disk, IP)
  start <name>                       Start a VM
  stop <name>                        Stop a VM
  restart <name>                     Restart a VM
  delete <name>                      Delete a VM
  ip <name>                          Get VM IP address
  snapshot create <name> <snap>      Create a snapshot
  snapshot list <name>               List snapshots for a VM
  snapshot restore <name> <snap>     Revert VM to a snapshot
  snapshot delete <name> <snap>      Delete a snapshot
  fork <source> <new-name> [options] Fork a VM from an external snapshot
  mount <name> <host-path> [options] Mount a host directory into the VM
  config <name> [options]            Reconfigure an existing VM

Options for 'create' and 'fork':
  --memory <size>                    Set memory (default: 1GiB)
  --vcpus <num>                      Set number of vCPUs (default: 2)
  --disk-size <size>                 Set disk size (default: 10G)
  --machine <type>                   Set machine type (default: pc-q35-10.0)
  --image <path>                     Use custom base image
  --no-start                         Create but don't start VM
  --no-wait-ip                       Don't wait for IP address
  --mount <host-path>[:<tag>]        Share a host directory (tag defaults to basename)

Options for 'config':
  --memory <size>                    Set memory (e.g. 2GiB, 512MiB)

Options for 'mount':
  --tag <tag>                        Mount tag visible inside the VM (default: basename)

Options for 'stop' and 'restart':
  --force                             Force stop (poweroff)

Options for 'delete':
  --force                             Force delete running VM

Global Options:
  --help, -h                         Show this help message
  --version, -v                      Show version information

Examples:
  vm create myvm
  vm create myvm --memory 2GiB --vcpus 4 --disk-size 20G
  vm create myvm --mount /home/user/projects
  vm create myvm --mount /home/user/projects:src
  vm create myvm --no-start
  vm fork myvm myvm-copy
  vm fork myvm myvm-copy --memory 2GiB --vcpus 4
  vm mount myvm /home/user/projects
  vm mount myvm /home/user/projects --tag src
  vm list
  vm start myvm
  vm ip myvm
"#
    );
}

fn print_version() {
    print!(
        "vm version {VERSION}\n\
         Copyright (C) 2022-26 evgnomon.org by Hamed Ghasemzadeh. All rights reserved.\n\
         License: HGL General License <https://evgnomon.org/docs/hgl>\n\
         There is NO warranty expressed or implied; to the extent permitted by law.\n"
    );
}

fn cmd_create(args: &[String], conn: &Connection, cfg: &Config) -> Result<()> {
    let domain_name = domain_arg(args, "vm create <name> [options]");
    let mut specs = VmSpecs::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--memory" => {
                let value = option_value(args, i, "--memory");
                specs.memory = parse_or_exit(parse_memory(value), "memory");
                i += 2;
            }
            "--vcpus" => {
                let value = option_value(args, i, "--vcpus");
                specs.vcpus = parse_or_exit(value.parse::<u32>(), "vcpus");
                i += 2;
            }
            "--machine" => {
                specs.machine = option_value(args, i, "--machine").to_string();
                i += 2;
            }
            "--image" => {
                specs.image_path = Some(option_value(args, i, "--image").to_string());
                i += 2;
            }
            "--disk-size" => {
                let value = option_value(args, i, "--disk-size");
                specs.disk_size = parse_or_exit(parse_disk_size(value), "disk-size");
                i += 2;
            }
            "--no-start" => {
                specs.start = false;
                i += 1;
            }
            "--no-wait-ip" => {
                specs.wait_for_ip = false;
                i += 1;
            }
            "--mount" => {
                specs
                    .mounts
                    .push(parse_mount(option_value(args, i, "--mount")));
                i += 2;
            }
            other => {
                error!("Error: unknown option: {other}");
                std::process::exit(1);
            }
        }
    }

    vm::create_vm(conn, cfg, domain_name, &specs)
}

fn cmd_fork(args: &[String], conn: &Connection, cfg: &Config) -> Result<()> {
    if args.len() < 2 {
        fail_usage(
            "Error: source and destination names required",
            "vm fork <source> <new-name> [options]",
        );
    }

    let source_name = args[0].as_str();
    let dest_name = args[1].as_str();
    let mut specs = VmSpecs::default();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--memory" => {
                let value = option_value(args, i, "--memory");
                specs.memory = parse_or_exit(parse_memory(value), "memory");
                i += 2;
            }
            "--vcpus" => {
                let value = option_value(args, i, "--vcpus");
                specs.vcpus = parse_or_exit(value.parse::<u32>(), "vcpus");
                i += 2;
            }
            "--no-start" => {
                specs.start = false;
                i += 1;
            }
            "--no-wait-ip" => {
                specs.wait_for_ip = false;
                i += 1;
            }
            other => {
                error!("Error: unknown option: {other}");
                std::process::exit(1);
            }
        }
    }

    vm::fork_vm(conn, cfg, source_name, dest_name, &specs)
}

fn cmd_mount(args: &[String], conn: &Connection) -> Result<()> {
    if args.len() < 2 {
        fail_usage(
            "Error: domain name and host path required",
            "vm mount <name> <host-path> [--tag <tag>]",
        );
    }

    let domain_name = args[0].as_str();
    let host_path = args[1].as_str();
    let mut tag = basename(host_path).to_string();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--tag" => {
                tag = option_value(args, i, "--tag").to_string();
                i += 2;
            }
            other => {
                error!("Error: unknown option: {other}");
                std::process::exit(1);
            }
        }
    }

    vm::mount_vm(
        conn,
        domain_name,
        &MountSpec {
            host_path: host_path.to_string(),
            tag,
        },
    )
}

fn cmd_config(args: &[String], conn: &Connection) -> Result<()> {
    let domain_name = domain_arg(args, "vm config <name> --memory <size>");
    let mut memory_kib: Option<u64> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--memory" => {
                let value = option_value(args, i, "--memory");
                memory_kib = Some(parse_or_exit(parse_memory(value), "memory"));
                i += 2;
            }
            other => {
                error!("Error: unknown option: {other}");
                std::process::exit(1);
            }
        }
    }

    let Some(memory_kib) = memory_kib else {
        error!("Error: at least one option required (e.g. --memory 2GiB)");
        std::process::exit(1);
    };

    vm::config_vm(conn, domain_name, memory_kib)
}

fn cmd_snapshot(args: &[String], conn: &Connection) -> Result<()> {
    const USAGE: &str = "vm snapshot <create|list|restore|delete> <name> [snapshot-name]";

    let Some(subcmd) = args.first().map(String::as_str) else {
        fail_usage("Error: snapshot subcommand required", USAGE);
    };

    let domain_and_snapshot = |usage: &str| -> (&str, &str) {
        if args.len() < 3 {
            fail_usage("Error: domain name and snapshot name required", usage);
        }
        (args[1].as_str(), args[2].as_str())
    };

    match subcmd {
        "create" => {
            let (name, snap) = domain_and_snapshot("vm snapshot create <name> <snapshot-name>");
            vm::create_snapshot(conn, name, snap)
        }
        "list" => {
            if args.len() < 2 {
                fail_usage("Error: domain name required", "vm snapshot list <name>");
            }
            vm::list_snapshots(conn, args[1].as_str())
        }
        "restore" => {
            let (name, snap) = domain_and_snapshot("vm snapshot restore <name> <snapshot-name>");
            vm::restore_snapshot(conn, name, snap)
        }
        "delete" => {
            let (name, snap) = domain_and_snapshot("vm snapshot delete <name> <snapshot-name>");
            vm::delete_snapshot(conn, name, snap)
        }
        other => {
            error!("Error: unknown snapshot subcommand: {other}");
            error!("Usage: {USAGE}");
            std::process::exit(1);
        }
    }
}

/// Splits `--mount <host-path>[:<tag>]`; the tag defaults to the path's basename.
fn parse_mount(value: &str) -> MountSpec {
    match value.rfind(':') {
        Some(pos) => MountSpec {
            host_path: value[..pos].to_string(),
            tag: value[pos + 1..].to_string(),
        },
        None => MountSpec {
            host_path: value.to_string(),
            tag: basename(value).to_string(),
        },
    }
}

fn basename(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

/// Parses a memory size into KiB; a bare number is already KiB.
fn parse_memory(value: &str) -> std::result::Result<u64, std::num::ParseIntError> {
    if let Some(base) = value.strip_suffix("GiB") {
        return Ok(base.parse::<u64>()? * 1024 * 1024);
    }
    if let Some(base) = value.strip_suffix("MiB") {
        return Ok(base.parse::<u64>()? * 1024);
    }
    if let Some(base) = value.strip_suffix('G') {
        return Ok(base.parse::<u64>()? * 1024 * 1024);
    }
    if let Some(base) = value.strip_suffix('M') {
        return Ok(base.parse::<u64>()? * 1024);
    }
    value.parse::<u64>()
}

/// Parses a disk size into bytes; a bare number is GiB.
fn parse_disk_size(value: &str) -> std::result::Result<u64, std::num::ParseIntError> {
    if let Some(base) = value.strip_suffix("GiB") {
        return Ok(base.parse::<u64>()? * 1024 * 1024 * 1024);
    }
    if let Some(base) = value.strip_suffix("MiB") {
        return Ok(base.parse::<u64>()? * 1024 * 1024);
    }
    if let Some(base) = value.strip_suffix('G') {
        return Ok(base.parse::<u64>()? * 1024 * 1024 * 1024);
    }
    if let Some(base) = value.strip_suffix('M') {
        return Ok(base.parse::<u64>()? * 1024 * 1024);
    }
    Ok(value.parse::<u64>()? * 1024 * 1024 * 1024)
}

#[cfg(test)]
mod tests {
    use super::{parse_disk_size, parse_memory, parse_mount};

    #[test]
    fn parses_memory_sizes_into_kib() {
        assert_eq!(parse_memory("2GiB").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_memory("512MiB").unwrap(), 512 * 1024);
        assert_eq!(parse_memory("2G").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_memory("512M").unwrap(), 512 * 1024);
        assert_eq!(parse_memory("4096").unwrap(), 4096);
        assert!(parse_memory("big").is_err());
    }

    #[test]
    fn parses_disk_sizes_into_bytes() {
        assert_eq!(parse_disk_size("20G").unwrap(), 20 * 1024 * 1024 * 1024);
        assert_eq!(parse_disk_size("20GiB").unwrap(), 20 * 1024 * 1024 * 1024);
        assert_eq!(parse_disk_size("512MiB").unwrap(), 512 * 1024 * 1024);
        // A bare number is treated as GiB.
        assert_eq!(parse_disk_size("10").unwrap(), 10 * 1024 * 1024 * 1024);
    }

    #[test]
    fn mount_tag_defaults_to_basename() {
        let mount = parse_mount("/home/user/projects");
        assert_eq!(mount.host_path, "/home/user/projects");
        assert_eq!(mount.tag, "projects");

        let mount = parse_mount("/home/user/projects:src");
        assert_eq!(mount.host_path, "/home/user/projects");
        assert_eq!(mount.tag, "src");
    }
}
