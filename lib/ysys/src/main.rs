// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

mod config;
mod resource;

use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};
use serde_json::Value;

use crate::resource::{Parsed, Resource, py_path, sql_text, text};

pub fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    process::exit(1)
}

/// ysys - sync resource YAMLs with PostgreSQL.
///
/// Connection settings come from the nearest .pg.json (searched from the
/// current directory upwards).
#[derive(Parser)]
#[command(name = "ysys")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Sync Kubernetes resource YAMLs from DIRECTORY into PostgreSQL.
    Sync {
        #[arg(value_parser = existing_dir)]
        directory: PathBuf,
        /// Tenant value written to every row
        #[arg(long, default_value = "system")]
        tenant: String,
        /// Parse files and print resources without writing to the database
        #[arg(long)]
        dry_run: bool,
    },
    /// Dump all resources from PostgreSQL into YAML files under DIRECTORY.
    ///
    /// Each resource is written to DIRECTORY/<tenant>/<namespace>/<kind>/<name>.yaml.
    Dump {
        #[arg(value_parser = not_a_file)]
        directory: PathBuf,
        /// Tenant to filter resources by
        #[arg(long, default_value = "system")]
        tenant: String,
        /// Filter by namespace (default: all namespaces)
        #[arg(long)]
        namespace: Option<String>,
    },
}

fn existing_dir(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(s);
    match (p.exists(), p.is_dir()) {
        (false, _) => Err(format!("Directory '{s}' does not exist.")),
        (true, false) => Err(format!("Directory '{s}' is a file.")),
        (true, true) => Ok(p),
    }
}

fn not_a_file(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(s);
    match p.exists() && !p.is_dir() {
        true => Err(format!("Directory '{s}' is a file.")),
        false => Ok(p),
    }
}

fn sync(directory: &Path, tenant: &str, dry_run: bool) {
    let mut resources: Vec<(PathBuf, Resource)> = Vec::new();
    for path in resource::find_yaml_files(directory) {
        let content = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("error reading {}: {e}", path.display());
                continue;
            }
        };
        let content = match String::from_utf8(content) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error reading {}: {e}", path.display());
                continue;
            }
        };
        let (docs, err) = resource::parse_stream(&content);
        for doc in docs {
            match doc {
                Parsed::NotMapping => {}
                Parsed::Incomplete => {
                    eprintln!("skip: missing apiVersion/kind/name in {}", path.display())
                }
                Parsed::Resource(r) => resources.push((path.clone(), *r)),
            }
        }
        if let Some(e) = err {
            eprintln!("error reading {}: {e}", path.display());
        }
    }

    println!("found {} resource(s)", resources.len());

    if dry_run {
        for (path, r) in &resources {
            println!(
                "  {}/{}  {}/{}  ({})",
                text(&r.api_version),
                text(&r.kind),
                text(&r.namespace),
                text(&r.name),
                path.display()
            );
        }
        return;
    }

    let mut client = config::connect();
    let mut tx = client.transaction().unwrap_or_else(|e| config::fail(e));
    for (_, r) in &resources {
        tx.execute(
            "INSERT INTO public.resources
                (api_version, kind, name, namespace, tenant, labels, annotations, spec)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (api_version, kind, name, namespace, tenant) DO UPDATE SET
                labels      = EXCLUDED.labels,
                annotations = EXCLUDED.annotations,
                spec        = EXCLUDED.spec",
            &[
                &sql_text(&r.api_version),
                &sql_text(&r.kind),
                &sql_text(&r.name),
                &sql_text(&r.namespace),
                &tenant,
                &r.labels,
                &r.annotations,
                &r.spec,
            ],
        )
        .unwrap_or_else(|e| config::fail(e));
        println!(
            "  upserted {}/{} {}/{}",
            text(&r.api_version),
            text(&r.kind),
            text(&r.namespace),
            text(&r.name)
        );
    }
    tx.commit().unwrap_or_else(|e| config::fail(e));
    println!("done.");
}

fn dump(directory: &Path, tenant: &str, namespace: Option<&str>) {
    let out_dir = py_path(directory);
    let mut client = config::connect();
    let rows = match namespace.filter(|n| !n.is_empty()) {
        Some(ns) => client.query(
            "SELECT api_version, kind, name, namespace, labels, annotations, spec
            FROM public.resources
            WHERE tenant = $1 AND namespace = $2
            ORDER BY namespace, kind, name",
            &[&tenant, &ns],
        ),
        None => client.query(
            "SELECT api_version, kind, name, namespace, labels, annotations, spec
            FROM public.resources
            WHERE tenant = $1
            ORDER BY namespace, kind, name",
            &[&tenant],
        ),
    }
    .unwrap_or_else(|e| config::fail(e));
    drop(client);

    println!(
        "dumping {} resource(s) to {}",
        rows.len(),
        out_dir.display()
    );

    for row in &rows {
        let s = |i: usize| row.get::<_, Option<String>>(i);
        let j = |i: usize| row.get::<_, Option<Value>>(i);
        let lower = |v: Option<String>| v.unwrap_or_else(|| "None".into()).to_lowercase();
        let dest = out_dir
            .join(tenant.to_lowercase())
            .join(lower(s(3)))
            .join(lower(s(1)))
            .join(format!("{}.yaml", lower(s(2))));
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|e| die(format!("{}: {e}", parent.display())));
        }
        let doc = resource::resource_to_doc(s(0), s(1), s(2), s(3), j(4), j(5), j(6));
        let yaml = serde_yaml::to_string(&doc).unwrap_or_else(|e| die(e));
        fs::write(&dest, yaml).unwrap_or_else(|e| die(format!("{}: {e}", dest.display())));
        println!("  wrote {}", dest.display());
    }

    println!("done.");
}

fn main() {
    match Cli::parse().command {
        Command::Sync {
            directory,
            tenant,
            dry_run,
        } => sync(&directory, &tenant, dry_run),
        Command::Dump {
            directory,
            tenant,
            namespace,
        } => dump(&directory, &tenant, namespace.as_deref()),
    }
}
