use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use postgres::error::SqlState;

use crate::config;
use crate::db::{connect, die, err_text, ident};

pub fn list() {
    let mut client = connect(None);
    let rows = client.query(
        "SELECT datname::text, pg_catalog.pg_get_userbyid(datdba)::text,
                pg_catalog.pg_encoding_to_char(encoding)::text
         FROM pg_catalog.pg_database
         WHERE datistemplate = false
         ORDER BY datname",
        &[],
    );
    match rows {
        Ok(rows) if rows.is_empty() => eprintln!("No databases found."),
        Ok(rows) => {
            eprintln!("Databases:");
            for row in rows {
                let s = |i| row.get::<_, String>(i);
                eprintln!("  • {} (owner: {}, encoding: {})", s(0), s(1), s(2));
            }
        }
        Err(e) => eprintln!("Error: {}", err_text(&e)),
    }
}

pub fn add(name: &str) {
    let mut client = connect(Some("postgres"));
    let exists = client
        .query_opt("SELECT 1 FROM pg_database WHERE datname = $1", &[&name])
        .unwrap_or_else(|e| die(format!("Error: {}", err_text(&e))));
    if exists.is_some() {
        eprintln!("Database '{name}' already exists.");
        return;
    }
    if let Err(e) = client.batch_execute(&format!("CREATE DATABASE {}", ident(name))) {
        die(format!("Error: {}", err_text(&e)));
    }
    eprintln!("Database '{name}' created successfully.");
}

pub fn drop(name: &str, force: bool) {
    let mut client = connect(Some("postgres"));
    if force {
        let terminated = client.execute(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
             WHERE datname = $1 AND pid <> pg_backend_pid()",
            &[&name],
        );
        if let Err(e) = terminated {
            die(format!("Error: {}", err_text(&e)));
        }
    }
    if let Err(e) = client.batch_execute(&format!("DROP DATABASE IF EXISTS {}", ident(name))) {
        die(format!("Error: {}", err_text(&e)));
    }
    eprintln!("Database '{name}' dropped.");
}

/// pg_dump/psql invocation with connection settings from config and env.
fn pg_tool(program: &str, dbname: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args([
        "-h",
        &config::host(),
        "-p",
        &config::port(),
        "-U",
        &config::user(),
        "-d",
        dbname,
    ])
    .env("PGPASSWORD", config::password());
    cmd
}

fn spawn_failed(program: &str, e: io::Error) -> ! {
    die(format!(
        "Error: failed to run {program}: {e}. Ensure it is installed."
    ))
}

pub fn save(name: &str, output: Option<PathBuf>) {
    let output = output.unwrap_or_else(|| PathBuf::from(format!("{name}.sql.gz")));
    let file = File::create(&output)
        .unwrap_or_else(|e| die(format!("Error: cannot create '{}': {e}", output.display())));

    let mut child = pg_tool("pg_dump", name)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| spawn_failed("pg_dump", e));
    let mut stdout = child.stdout.take().expect("piped stdout");

    let mut gz = GzEncoder::new(BufWriter::new(file), Compression::default());
    let copied = io::copy(&mut stdout, &mut gz)
        .and_then(|_| gz.finish()?.into_inner().map_err(|e| e.into_error()));
    let status = child.wait().unwrap_or_else(|e| spawn_failed("pg_dump", e));

    if !status.success() {
        let _ = fs::remove_file(&output);
        die(format!(
            "Error: pg_dump failed with exit code {}",
            status.code().unwrap_or(-1)
        ));
    }
    if let Err(e) = copied {
        die(format!("Error: writing '{}': {e}", output.display()));
    }
    eprintln!("Database '{name}' saved to '{}'.", output.display());
}

fn infer_name(input: &Path) -> String {
    let base = input
        .file_name()
        .map(|b| b.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = base.split('.').next().unwrap_or_default().to_string();
    if name.is_empty() {
        die("Error: Could not infer database name from filename. Use --name.");
    }
    name
}

fn create_db(name: &str) {
    let mut client = connect(Some("postgres"));
    match client.batch_execute(&format!("CREATE DATABASE {}", ident(name))) {
        Ok(()) => eprintln!("Database '{name}' created."),
        Err(e) if e.code() == Some(&SqlState::DUPLICATE_DATABASE) => {
            eprintln!("Database '{name}' already exists, restoring into it.")
        }
        Err(e) => die(format!("Error creating database: {}", err_text(&e))),
    }
}

pub fn restore(input: &Path, name: Option<String>, create: bool) {
    let file = File::open(input)
        .unwrap_or_else(|e| die(format!("Error: cannot open '{}': {e}", input.display())));
    let name = name.unwrap_or_else(|| infer_name(input));
    if create {
        create_db(&name);
    }

    let mut child = pg_tool("psql", &name)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| spawn_failed("psql", e));

    let mut stdin = child.stdin.take().expect("piped stdin");
    let feeder = thread::spawn(move || {
        let mut gz = MultiGzDecoder::new(BufReader::new(file));
        io::copy(&mut gz, &mut stdin)
    });

    let mut stderr = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }
    let status = child.wait().unwrap_or_else(|e| spawn_failed("psql", e));
    let fed = feeder
        .join()
        .unwrap_or_else(|_| die("Error: decompression thread panicked"));

    if !status.success() {
        die(format!("Error: psql failed: {}", stderr.trim()));
    }
    if let Err(e) = fed {
        die(format!("Error: reading '{}': {e}", input.display()));
    }
    eprintln!("Database '{name}' restored from '{}'.", input.display());
}
