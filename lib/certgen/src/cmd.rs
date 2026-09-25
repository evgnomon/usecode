// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Subcommand implementations.

use std::fs;
use std::io::{BufRead, Write};

use anyhow::{Context, Result};

use crate::cert::{self, CertSpec, CertType};
use crate::store::{Store, arg, openssl};

fn remove_if_exists(path: &std::path::Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).with_context(|| format!("removing {}", path.display()))
        }
        _ => Ok(()),
    }
}

pub fn init(store: &Store, days: i64, key_size: i64, force: bool) -> Result<()> {
    if force && store.ca_exists() {
        println!("Removing existing CA...");
        remove_if_exists(&store.ca_key)?;
        remove_if_exists(&store.ca_cert)?;
        remove_if_exists(&store.ca_serial)?;
    }
    cert::create_ca(store, days, key_size)
}

/// Domains and IPs for a server certificate: `localhost` when no domain is
/// given, plus `127.0.0.1` when neither domains nor IPs are given.
pub fn server_sans(domain: Vec<String>, ip: Vec<String>) -> (Vec<String>, Vec<String>) {
    if domain.is_empty() && ip.is_empty() {
        return (vec!["localhost".into()], vec!["127.0.0.1".into()]);
    }
    let domains = if domain.is_empty() {
        vec!["localhost".into()]
    } else {
        domain
    };
    (domains, ip)
}

#[allow(clippy::too_many_arguments)]
pub fn server(
    store: &Store,
    name: &str,
    domain: Vec<String>,
    ip: Vec<String>,
    days: i64,
    key_size: i64,
    p12: bool,
    p12_password: &str,
) -> Result<()> {
    let (domains, ips) = server_sans(domain, ip);
    cert::create_certificate(
        store,
        &CertSpec {
            name,
            domains: &domains,
            ips: &ips,
            cert_type: CertType::Server,
            days,
            key_size,
            create_p12: p12,
            p12_password,
        },
    )
}

pub fn client(
    store: &Store,
    name: &str,
    email: Option<String>,
    days: i64,
    key_size: i64,
    p12: bool,
    p12_password: &str,
) -> Result<()> {
    let domains = vec![
        email
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| name.into()),
    ];
    cert::create_certificate(
        store,
        &CertSpec {
            name,
            domains: &domains,
            ips: &[],
            cert_type: CertType::Client,
            days,
            key_size,
            create_p12: p12,
            p12_password,
        },
    )
}

pub fn list(store: &Store, verbose: bool) -> Result<()> {
    if !store.dir.exists() {
        println!("No certificates found. Run 'cert-gen init' first.");
        return Ok(());
    }

    let mut certs: Vec<_> = fs::read_dir(&store.dir)
        .with_context(|| format!("reading {}", store.dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pub"))
        .collect();
    certs.sort();

    if certs.is_empty() {
        println!("No certificates found.");
        return Ok(());
    }

    println!("Certificates in {}:\n", store.dir.display());

    for cert_path in certs {
        let name = cert_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_ca = name == "ca";

        if verbose {
            let out = openssl(
                &[
                    "x509",
                    "-in",
                    &arg(&cert_path),
                    "-noout",
                    "-subject",
                    "-issuer",
                    "-dates",
                    "-ext",
                    "subjectAltName",
                ],
                false,
            )?;
            let bar = "=".repeat(50);
            println!("{bar}");
            println!("Certificate: {name} {}", if is_ca { "(CA)" } else { "" });
            println!("{bar}");
            println!("{}", String::from_utf8_lossy(&out.stdout));
        } else {
            let out = openssl(
                &["x509", "-in", &arg(&cert_path), "-noout", "-enddate"],
                false,
            )?;
            let expiry = String::from_utf8_lossy(&out.stdout)
                .trim()
                .replace("notAfter=", "");
            let mut extras = Vec::new();
            if store.file(&name, "key").exists() {
                extras.push("key");
            }
            if store.file(&name, "p12").exists() {
                extras.push("p12");
            }
            let status = if is_ca { "CA" } else { "cert" };
            println!(
                "  {name:20} [{status}] expires: {expiry} ({})",
                extras.join(", ")
            );
        }
    }
    Ok(())
}

pub fn show(store: &Store, name: &str) -> Result<i32> {
    let cert_path = store.file(name, "pub");
    if !cert_path.exists() {
        println!("Certificate not found: {}", cert_path.display());
        return Ok(1);
    }
    let out = openssl(&["x509", "-in", &arg(&cert_path), "-noout", "-text"], true)?;
    println!("{}", String::from_utf8_lossy(&out.stdout));
    Ok(0)
}

pub fn verify(store: &Store, name: &str) -> Result<i32> {
    let cert_path = store.file(name, "pub");
    if !cert_path.exists() {
        println!("Certificate not found: {}", cert_path.display());
        return Ok(1);
    }
    if !store.ca_exists() {
        println!("CA not found. Cannot verify.");
        return Ok(1);
    }
    let out = openssl(
        &["verify", "-CAfile", &arg(&store.ca_cert), &arg(&cert_path)],
        false,
    )?;
    if out.status.success() {
        println!("✓ Certificate {name} is valid and signed by the CA");
    } else {
        println!("✗ Certificate {name} verification failed:");
        println!("{}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(0)
}

pub fn export(store: &Store, name: &str, password: &str) -> Result<i32> {
    let cert_path = store.file(name, "pub");
    let key_path = store.file(name, "key");
    let p12_path = store.file(name, "p12");

    if !cert_path.exists() {
        println!("Certificate not found: {}", cert_path.display());
        return Ok(1);
    }
    if !key_path.exists() {
        println!("Private key not found: {}", key_path.display());
        return Ok(1);
    }

    cert::export_p12(store, &key_path, &cert_path, &p12_path, password)?;
    println!("✓ Exported to: {}", p12_path.display());
    println!("  Password: {password}");
    Ok(0)
}

/// Parse a click-style confirmation answer (default: no).
fn parse_confirm(answer: &str) -> Option<bool> {
    match answer.trim().to_lowercase().as_str() {
        "" | "n" | "no" | "f" | "false" | "0" => Some(false),
        "y" | "yes" | "t" | "true" | "1" => Some(true),
        _ => None,
    }
}

/// Ask `prompt [y/N]: ` until a valid answer; `None` on EOF.
fn confirm(prompt: &str) -> Result<Option<bool>> {
    let stdin = std::io::stdin();
    loop {
        print!("{prompt} [y/N]: ");
        std::io::stdout().flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            println!();
            return Ok(None);
        }
        match parse_confirm(&line) {
            Some(v) => return Ok(Some(v)),
            None => eprintln!("Error: invalid input"),
        }
    }
}

pub fn delete(store: &Store, name: &str, force: bool) -> Result<i32> {
    if name == "ca" {
        println!("Cannot delete CA using this command. Use 'cert-gen init --force' to recreate.");
        return Ok(1);
    }

    let existing: Vec<_> = ["key", "pub", "p12"]
        .iter()
        .map(|ext| store.file(name, ext))
        .filter(|p| p.exists())
        .collect();

    if existing.is_empty() {
        println!("No files found for certificate: {name}");
        return Ok(1);
    }

    if !force {
        println!("Will delete:");
        for f in &existing {
            println!("  {}", f.display());
        }
        match confirm("Continue?")? {
            None => {
                eprintln!("Aborted!");
                return Ok(1);
            }
            Some(false) => {
                println!("Aborted.");
                return Ok(0);
            }
            Some(true) => {}
        }
    }

    for f in existing {
        fs::remove_file(&f).with_context(|| format!("removing {}", f.display()))?;
        println!("Deleted: {}", f.display());
    }
    Ok(0)
}

/// nginx server block snippet for a certificate.
pub fn nginx_snippet(name: &str, server_name: &str, cert: &str, key: &str) -> String {
    format!(
        r#"# nginx SSL configuration for {name}
# Add this to your server block

server {{
    listen 443 ssl http2;
    server_name {server_name};

    ssl_certificate {cert};
    ssl_certificate_key {key};

    # Modern SSL settings
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
    ssl_prefer_server_ciphers off;
    ssl_session_timeout 1d;
    ssl_session_cache shared:SSL:10m;

    # Your location blocks here
    location / {{
        root /var/www/html;
        index index.html;
    }}
}}

# Optional: Redirect HTTP to HTTPS
server {{
    listen 80;
    server_name {server_name};
    return 301 https://$server_name$request_uri;
}}
"#
    )
}

pub fn nginx_config(store: &Store, name: &str, server_name: Option<&str>) -> Result<i32> {
    let cert_path = store.file(name, "pub");
    let key_path = store.file(name, "key");
    if !cert_path.exists() {
        println!("Certificate not found: {}", cert_path.display());
        return Ok(1);
    }
    let sn = server_name.filter(|s| !s.is_empty()).unwrap_or(name);
    println!(
        "{}",
        nginx_snippet(name, sn, &arg(&cert_path), &arg(&key_path))
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sans() {
        assert_eq!(
            server_sans(vec![], vec![]),
            (vec!["localhost".to_string()], vec!["127.0.0.1".to_string()])
        );
        assert_eq!(
            server_sans(vec![], vec!["10.0.0.1".into()]),
            (vec!["localhost".to_string()], vec!["10.0.0.1".to_string()])
        );
        assert_eq!(
            server_sans(vec!["a.com".into()], vec![]),
            (vec!["a.com".to_string()], vec![])
        );
    }

    #[test]
    fn confirm_answers() {
        assert_eq!(parse_confirm("\n"), Some(false));
        assert_eq!(parse_confirm("Y\n"), Some(true));
        assert_eq!(parse_confirm("yes"), Some(true));
        assert_eq!(parse_confirm("no"), Some(false));
        assert_eq!(parse_confirm("maybe"), None);
    }

    #[test]
    fn nginx_snippet_substitutes() {
        let s = nginx_snippet("app", "app.local", "/x/app.pub", "/x/app.key");
        assert!(s.starts_with("# nginx SSL configuration for app\n"));
        assert!(s.contains("    server_name app.local;\n"));
        assert!(s.contains("    ssl_certificate /x/app.pub;\n"));
        assert!(s.contains("return 301 https://$server_name$request_uri;"));
        assert!(s.ends_with("}\n"));
    }
}
