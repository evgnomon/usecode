// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! mkpod — manage pods via podman from a PostgreSQL resource store.

mod db;
mod podman;
mod spec;

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use postgres::Client;

/// mkpod — manage pods via podman from a PostgreSQL resource store.
#[derive(Parser)]
#[command(name = "mkpod", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args)]
struct DbOpts {
    /// DB host
    #[arg(long, env = "PG_HOST", default_value = "localhost")]
    host: String,
    /// DB port
    #[arg(long, env = "PG_PORT", default_value_t = 5432)]
    port: u16,
    /// DB user
    #[arg(long, env = "PG_USER", default_value = "admin")]
    user: String,
    /// DB password
    #[arg(
        long,
        env = "PG_PASSWORD",
        default_value = "t6drtfyig7",
        hide_default_value = true,
        hide_env_values = true
    )]
    password: String,
    /// DB name
    #[arg(long, env = "PG_DATABASE", default_value = "z")]
    database: String,
}

#[derive(Args)]
struct NsOpts {
    /// Namespace
    #[arg(long, short = 'n', default_value = "main")]
    namespace: String,
    /// Tenant
    #[arg(long, short = 't', default_value = "system")]
    tenant: String,
}

#[derive(Args)]
struct Scope {
    #[command(flatten)]
    db: DbOpts,
    #[command(flatten)]
    ns: NsOpts,
}

impl Scope {
    fn connect(&self) -> Result<Client> {
        let d = &self.db;
        db::connect(&db::Conn {
            host: d.host.clone(),
            port: d.port,
            user: d.user.clone(),
            password: d.password.clone(),
            database: d.database.clone(),
        })
    }
}

#[derive(Subcommand)]
enum Command {
    /// Create or replace pods from the database using podman kube play.
    ///
    /// If POD_NAME is given, only that pod is applied; otherwise all pods
    /// in the namespace/tenant are applied. Orphaned pods are cleaned up
    /// after a successful run.
    Apply {
        #[command(flatten)]
        scope: Scope,
        pod_name: Option<String>,
    },
    /// List pods stored in the database.
    List {
        #[command(flatten)]
        scope: Scope,
    },
    /// Remove podman pods whose resources have been deleted from the database.
    Cleanup {
        #[command(flatten)]
        scope: Scope,
    },
    /// Show all podman pods currently managed by mkpod.
    Managed,
}

/// Create one pod (and its ConfigMaps/PVCs). Returns true on success.
fn create_pod(client: &mut Client, pod: &spec::Pod, ns: &NsOpts) -> Result<bool> {
    let name = pod.name();
    let cms = db::ensure_configmaps(client, &pod.doc, &ns.namespace, &ns.tenant)?;
    let pvcs = db::ensure_pvcs(client, &pod.doc, &ns.namespace, &ns.tenant)?;
    Ok(podman::kube_play(&name, pvcs, pod.doc.clone(), &cms))
}

/// Remove podman pods whose resource row no longer exists.
fn remove_orphaned_pods(client: &mut Client, ns: &NsOpts) -> Result<usize> {
    let managed = podman::managed_pods()?;
    if managed.is_empty() {
        return Ok(0);
    }
    let active = db::active_pod_ids(client, &ns.namespace, &ns.tenant)?;
    let mut removed = 0;
    for (id, name) in managed.iter().filter(|(id, _)| !active.contains(id)) {
        println!("Removing orphaned pod '{name}' (resource {id})...");
        match podman::remove_pod(name)? {
            Ok(()) => {
                println!("Removed orphaned pod '{name}'");
                removed += 1;
            }
            Err(stderr) => eprintln!("Failed to remove pod '{name}': {stderr}"),
        }
    }
    Ok(removed)
}

fn apply(scope: &Scope, pod_name: Option<&str>) -> Result<()> {
    let mut client = scope.connect()?;
    let mut pods = db::get_pods(&mut client, &scope.ns.namespace, &scope.ns.tenant)?;
    if pods.is_empty() {
        println!("No pods found in database.");
        return Ok(());
    }
    if let Some(wanted) = pod_name {
        pods.retain(|p| p.doc["metadata"]["name"].as_str() == Some(wanted));
        if pods.is_empty() {
            bail!("Pod '{wanted}' not found.");
        }
    }
    let mut success = 0;
    for pod in &pods {
        success += usize::from(create_pod(&mut client, pod, &scope.ns)?);
    }
    println!("\nApplied {success}/{} pod(s) successfully.", pods.len());
    remove_orphaned_pods(&mut client, &scope.ns)?;
    Ok(())
}

fn list(scope: &Scope) -> Result<()> {
    let mut client = scope.connect()?;
    let rows = db::list_pods(&mut client, &scope.ns.namespace, &scope.ns.tenant)?;
    if rows.is_empty() {
        println!("No pods found in database.");
        return Ok(());
    }
    println!("Found {} pod(s):\n", rows.len());
    for (name, spec) in rows {
        println!("  {}", name.as_deref().unwrap_or("None"));
        let containers = spec
            .as_ref()
            .and_then(|s| s.get("containers"))
            .and_then(|c| c.as_array());
        for c in containers.into_iter().flatten() {
            let field =
                |k, default: &str| c.get(k).map_or_else(|| default.to_string(), spec::py_str);
            println!("    container: {}", field("name", "unnamed"));
            println!("    image:     {}", field("image", "none"));
        }
    }
    Ok(())
}

fn cleanup(scope: &Scope) -> Result<()> {
    let mut client = scope.connect()?;
    match remove_orphaned_pods(&mut client, &scope.ns)? {
        0 => println!("No orphaned pods found."),
        n => println!("Removed {n} orphaned pod(s)."),
    }
    Ok(())
}

fn managed() -> Result<()> {
    let mut pods = podman::managed_pods()?;
    if pods.is_empty() {
        println!("No mkpod-managed pods running.");
        return Ok(());
    }
    pods.sort();
    println!("{:<40} POD NAME", "RESOURCE ID");
    println!("{}", "-".repeat(60));
    for (id, name) in pods {
        println!("{id:<40} {name}");
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let res = match &cli.command {
        Command::Apply { scope, pod_name } => apply(scope, pod_name.as_deref()),
        Command::List { scope } => list(scope),
        Command::Cleanup { scope } => cleanup(scope),
        Command::Managed => managed(),
    };
    if let Err(e) = res {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}
