use std::fs;
use std::process::{Command, Stdio};

use clap::Args;

use crate::config::CFG;
use crate::db::die;

#[derive(Args)]
pub struct Opts {
    /// Container name
    #[arg(short, long, default_value = CFG.container.as_str())]
    name: String,
    /// Host port
    #[arg(short, long, default_value_t = CFG.port)]
    port: u16,
    /// PostgreSQL image
    #[arg(long, default_value = CFG.image.as_str())]
    image: String,
    /// Data directory
    #[arg(long, default_value = CFG.data_dir.as_str())]
    data_dir: String,
    /// Podman network
    #[arg(long, default_value = CFG.network.as_str())]
    network: String,
    /// Pull image before starting (default)
    #[arg(long, overrides_with = "no_pull")]
    pull: bool,
    /// Do not pull the image
    #[arg(long, overrides_with = "pull")]
    no_pull: bool,
    /// Remove existing container and start fresh
    #[arg(long)]
    force: bool,
}

/// Run podman quietly; true on success.
fn podman(args: &[&str]) -> bool {
    Command::new("podman")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn container_state(name: &str) -> Option<String> {
    let filter = format!("name=^{name}$");
    let out = Command::new("podman")
        .args(["ps", "-a", "--filter", &filter, "--format", "{{.State}}"])
        .output()
        .ok()?;
    let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!state.is_empty()).then_some(state)
}

fn ensure_network(name: &str) {
    if podman(&["network", "exists", name]) {
        return;
    }
    eprintln!("Network '{name}' does not exist. Creating...");
    if !podman(&["network", "create", name]) {
        die(format!("Error: Failed to create network '{name}'."));
    }
    eprintln!("Network '{name}' created successfully.");
}

pub fn run(o: &Opts) {
    let name = o.name.as_str();
    let state = container_state(name);

    match (&state, o.force) {
        (Some(s), false) if s == "running" => {
            eprintln!("Container '{name}' is already running.");
            return;
        }
        (Some(s), false) => {
            eprintln!("Container '{name}' exists (state: {s}). Starting...");
            if !podman(&["start", name]) {
                die(format!("Error: Failed to start container '{name}'."));
            }
            eprintln!("Container '{name}' started.");
            return;
        }
        (Some(_), true) => {
            eprintln!("Removing existing container '{name}'...");
            podman(&["rm", "-f", name]);
        }
        (None, _) => {}
    }

    ensure_network(&o.network);

    if !o.no_pull {
        eprintln!("Pulling image {}...", o.image);
        podman(&["pull", &o.image]);
    }

    if let Err(e) = fs::create_dir_all(&o.data_dir) {
        die(format!(
            "Error: cannot create data directory '{}': {e}",
            o.data_dir
        ));
    }

    let port_map = format!("{}:5432", o.port);
    let volume = format!("{}:/var/lib/postgresql/data", o.data_dir);
    let password = format!("POSTGRES_PASSWORD={}", CFG.password);
    let user = format!("POSTGRES_USER={}", CFG.user);

    eprintln!(
        "Starting PostgreSQL container '{name}' on port {}...",
        o.port
    );
    let out = Command::new("podman")
        .args(["run", "-d", "--name", name, "--hostname", name])
        .args(["--network", &o.network, "--restart", "unless-stopped"])
        .args(["-p", &port_map, "-v", &volume])
        .args(["-e", &password, "-e", &user, &o.image])
        .output()
        .unwrap_or_else(|e| die(format!("Error starting container: {e}")));
    if !out.status.success() {
        die(format!(
            "Error starting container: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    eprintln!("PostgreSQL container '{name}' started successfully.");
    eprintln!("\nQuick commands:");
    eprintln!("  Logs   : podman logs -f {name}");
    eprintln!("  Stop   : podman stop {name}");
    eprintln!("  Remove : podman rm -f {name}");
    eprintln!("  Connect: psql -h localhost -p {} -U {}", o.port, CFG.user);
}
