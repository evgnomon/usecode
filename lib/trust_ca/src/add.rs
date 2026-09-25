// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `trust_ca <host:port>`: fetch the serving CA and install it.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pem::{extract_blocks, is_ca, split_bundle, x509};
use crate::sh::{capture, has_cmd, run};
use crate::{CONTAINERS_DIR, SYSTEM_DIR, cert_name, host_of};

/// Temporary directory removed on drop (the script's `trap ... EXIT`).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("trust-ca-{}{:08x}", std::process::id(), nanos));
        fs::create_dir(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn io_err(e: std::io::Error) -> i32 {
    eprintln!("Error: {e}");
    1
}

pub fn add(registry: &str) -> i32 {
    match try_add(registry) {
        Ok(()) => 0,
        Err(code) => code,
    }
}

fn try_add(registry: &str) -> Result<(), i32> {
    let host = host_of(registry);
    let name = cert_name(host);
    let tmp = TempDir::new().map_err(io_err)?;

    println!("Fetching certificate chain from {registry} ...");
    let (ok, out) = capture(
        "openssl",
        &[
            "s_client",
            "-connect",
            registry,
            "-servername",
            host,
            "-showcerts",
        ],
        None,
    );
    let chain = extract_blocks(&out);
    if !ok || chain.is_empty() {
        println!("Error: failed to retrieve certificate from {registry}");
        return Err(1);
    }
    fs::write(tmp.0.join("full-chain.pem"), &chain).map_err(io_err)?;

    // Split chain into individual certs and keep only CA certs (skip the leaf).
    let certs = split_bundle(&chain);
    let mut cert_files = Vec::new();
    for (i, cert) in certs.iter().enumerate() {
        let path = tmp.0.join(format!("cert-{i:02}.pem"));
        fs::write(&path, cert).map_err(io_err)?;
        cert_files.push(path);
    }

    let mut bundle = String::new();
    let mut count = 0;
    let mut ca_flags = Vec::new();
    for cert in &certs {
        let subject = x509(cert, "-subject").unwrap_or_default();
        let issuer = x509(cert, "-issuer").unwrap_or_default();
        let ca = is_ca(cert);
        ca_flags.push(ca);
        if ca {
            println!("  Found CA: {subject}");
            println!("            {issuer}");
            bundle.push_str(cert);
            count += 1;
        } else {
            println!("  Skipping leaf: {subject}");
        }
    }

    // If no CA certs found in chain, take everything except the first (leaf) cert.
    if count == 0 {
        println!("  No CA:TRUE certs found, using all non-leaf certs from chain...");
        for cert in certs.iter().skip(1) {
            bundle.push_str(cert);
            count += 1;
        }
    }

    if bundle.is_empty() {
        println!("Error: could not extract any CA certificates from the chain");
        return Err(1);
    }
    let ca_bundle = tmp.0.join("ca-bundle.pem");
    fs::write(&ca_bundle, &bundle).map_err(io_err)?;
    let ca_bundle = ca_bundle.to_string_lossy();

    println!();
    println!("Extracted {count} CA certificate(s).");

    println!();
    println!("==> Adding to system CA trust store ...");
    run(
        "sudo",
        &["cp", &ca_bundle, &format!("{SYSTEM_DIR}/{name}.crt")],
    )?;
    run("sudo", &["update-ca-certificates"])?;

    println!();
    println!("==> Adding to container runtime trust store ...");
    let container_dir = format!("{CONTAINERS_DIR}/{registry}");
    run("sudo", &["mkdir", "-p", &container_dir])?;
    run(
        "sudo",
        &["cp", &ca_bundle, &format!("{container_dir}/ca.crt")],
    )?;

    println!();
    if has_cmd("certutil") {
        println!("==> Adding to NSS database (Brave/Chrome) ...");
        let home = std::env::var("HOME").unwrap_or_default();
        fs::create_dir_all(Path::new(&home).join(".pki/nssdb")).map_err(io_err)?;
        let db = format!("sql:{home}/.pki/nssdb");
        let ca_files = cert_files.iter().zip(&ca_flags).filter(|(_, ca)| **ca);
        for (i, (file, _)) in ca_files.enumerate() {
            let nick = format!("{name}-{i}");
            let file = file.to_string_lossy();
            run(
                "certutil",
                &["-d", &db, "-A", "-t", "CT,C,C", "-n", &nick, "-i", &file],
            )?;
        }
    } else {
        println!("==> Skipping NSS database (certutil not found — install libnss3-tools)");
    }

    println!();
    println!("Done. Restart Brave/Chrome for browser changes to take effect.");
    Ok(())
}
