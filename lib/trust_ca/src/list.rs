// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `trust_ca --list`: show custom-trusted CAs across the three stores.

use std::fs;
use std::path::{Path, PathBuf};

use regex::RegexBuilder;

use crate::pem::{nickname, split_bundle, x509};
use crate::sh::{capture, has_cmd, modified, trim_nl};
use crate::{CONTAINERS_DIR, SYSTEM_DIR};

/// Non-hidden entries of `dir`, sorted, like a shell glob.
fn glob(dir: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn base(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn strip<'a>(s: &'a str, prefix: &str) -> &'a str {
    s.strip_prefix(prefix).unwrap_or(s)
}

fn print_cert_info(file: &Path, store: &str) {
    println!("  Store:    {store}");
    println!("  File:     {}", file.display());
    println!("  Modified: {}", modified(file));

    let text = fs::read(file)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    for cert in split_bundle(&text) {
        let subj = x509(&cert, "-subject").unwrap_or_else(|| "  (unknown)".into());
        let issuer = x509(&cert, "-issuer").unwrap_or_else(|| "  (unknown)".into());
        let start = x509(&cert, "-startdate").unwrap_or_default();
        let end = x509(&cert, "-enddate").unwrap_or_default();
        println!("  ---");
        println!("  {subj}");
        println!("  {issuer}");
        println!(
            "  Valid: {} — {}",
            strip(&start, "notBefore="),
            strip(&end, "notAfter=")
        );
    }
    println!();
}

fn none() {
    println!("  (none)");
    println!();
}

pub fn list_certs(filter: &str) {
    let mut found = false;
    println!("=== System CA Trust Store ===");
    println!("    /usr/local/share/ca-certificates/");
    println!();
    for f in glob(SYSTEM_DIR) {
        let name = base(&f);
        if !name.ends_with(".crt") || !f.is_file() || !name.contains(filter) {
            continue;
        }
        print_cert_info(&f, "system");
        found = true;
    }
    if !found {
        none();
    }

    found = false;
    println!("=== Container Runtime (Podman/Skopeo) ===");
    println!("    /etc/containers/certs.d/");
    println!();
    for d in glob(CONTAINERS_DIR) {
        let name = base(&d);
        if !d.is_dir() || !name.contains(filter) {
            continue;
        }
        let ca_file = d.join("ca.crt");
        if ca_file.is_file() {
            println!("  Registry: {name}");
            print_cert_info(&ca_file, "container");
            found = true;
        }
    }
    if !found {
        none();
    }

    found = false;
    println!("=== NSS Database (Brave/Chrome) ===");
    println!("    ~/.pki/nssdb");
    println!();
    let home = std::env::var("HOME").unwrap_or_default();
    let db = format!("sql:{home}/.pki/nssdb");
    let certutil = has_cmd("certutil");
    if certutil && Path::new(&format!("{home}/.pki/nssdb")).is_dir() {
        found = list_nss(&db, filter);
    }
    if !found {
        if certutil {
            println!("  (none)");
        } else {
            println!("  (certutil not found — install libnss3-tools)");
        }
        println!();
    }
}

/// Print matching NSS entries; returns whether anything matched.
fn list_nss(db: &str, filter: &str) -> bool {
    let filter_nss = filter.replace('.', "-");
    let matcher = RegexBuilder::new(&filter_nss)
        .case_insensitive(true)
        .build()
        .ok();
    let matches = |s: &str| filter.is_empty() || matcher.as_ref().is_some_and(|m| m.is_match(s));

    let (_, listing) = capture("certutil", &["-d", db, "-L"], None);
    let listing = trim_nl(&listing);
    let shown: Vec<&str> = if listing.is_empty() {
        Vec::new()
    } else {
        listing.split('\n').filter(|l| matches(l)).collect()
    };
    if shown.is_empty() {
        return false;
    }
    for line in &shown {
        println!("  {line}");
    }
    println!();

    // Detailed info for matching certs (skip the 4-line header).
    for line in listing.split('\n').skip(4) {
        if line.is_empty() {
            continue;
        }
        let nick = nickname(line);
        if nick.is_empty() || !matches(&nick) {
            continue;
        }
        let (_, out) = capture("certutil", &["-d", db, "-L", "-n", &nick], None);
        let detail: Vec<&str> = out
            .lines()
            .filter(|l| {
                ["Subject:", "Issuer:", "Not Before:", "Not After"]
                    .iter()
                    .any(|k| l.contains(k))
            })
            .collect();
        if !detail.is_empty() {
            println!("  [{nick}]");
            for d in detail {
                println!("    {d}");
            }
            println!();
        }
    }
    true
}
