// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! CA and leaf certificate creation (shells out to openssl).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result};

use crate::store::{Store, arg, openssl};

/// Kind of leaf certificate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CertType {
    Server,
    Client,
}

impl CertType {
    pub fn as_str(self) -> &'static str {
        match self {
            CertType::Server => "server",
            CertType::Client => "client",
        }
    }

    /// Organizational unit used in the subject.
    pub fn ou(self) -> &'static str {
        match self {
            CertType::Server => "Server",
            CertType::Client => "Client",
        }
    }
}

/// Parameters for [`create_certificate`].
pub struct CertSpec<'a> {
    pub name: &'a str,
    pub domains: &'a [String],
    pub ips: &'a [String],
    pub cert_type: CertType,
    pub days: i64,
    pub key_size: i64,
    pub create_p12: bool,
    pub p12_password: &'a str,
}

fn chmod(path: &Path, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("chmod {}", path.display()))
}

/// Create a new Certificate Authority if it doesn't exist.
pub fn create_ca(store: &Store, days: i64, key_size: i64) -> Result<()> {
    if store.ca_exists() {
        println!(
            "CA already exists at {} and {}",
            store.ca_key.display(),
            store.ca_cert.display()
        );
        return Ok(());
    }

    store.ensure_dir()?;

    println!("Creating new Certificate Authority...");

    openssl(
        &[
            "req".to_string(),
            "-x509".into(),
            "-newkey".into(),
            format!("rsa:{key_size}"),
            "-sha256".into(),
            "-days".into(),
            days.to_string(),
            "-nodes".into(),
            "-keyout".into(),
            arg(&store.ca_key),
            "-out".into(),
            arg(&store.ca_cert),
            "-subj".into(),
            "/CN=Local Development CA/O=cert-gen/OU=Development".into(),
            "-addext".into(),
            "basicConstraints = critical,CA:TRUE".into(),
            "-addext".into(),
            "keyUsage = critical,keyCertSign,cRLSign".into(),
        ],
        true,
    )?;
    chmod(&store.ca_key, 0o600)?;

    fs::write(&store.ca_serial, "1000\n")
        .with_context(|| format!("writing {}", store.ca_serial.display()))?;

    let ca_key = store.ca_key.display();
    let ca_cert = store.ca_cert.display();
    println!("✓ CA private key: {ca_key}");
    println!("✓ CA certificate: {ca_cert}");
    println!("\nTo trust this CA in Chrome/system:");
    println!(
        "  - Linux: sudo cp {ca_cert} /usr/local/share/ca-certificates/cert-gen-ca.crt && sudo update-ca-certificates"
    );
    println!(
        "  - macOS: sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain {ca_cert}"
    );
    println!(
        "  - Chrome: Settings > Privacy > Security > Manage certificates > Authorities > Import {ca_cert}"
    );
    Ok(())
}

/// Content of the openssl extension file for a leaf certificate.
pub fn ext_content(cert_type: CertType, domains: &[String], ips: &[String]) -> String {
    let mut lines: Vec<String> = match cert_type {
        CertType::Server => vec![
            "basicConstraints = CA:FALSE".into(),
            "keyUsage = critical,digitalSignature,keyEncipherment".into(),
            "extendedKeyUsage = serverAuth".into(),
        ],
        CertType::Client => vec![
            "basicConstraints = CA:FALSE".into(),
            "keyUsage = critical,digitalSignature".into(),
            "extendedKeyUsage = clientAuth".into(),
        ],
    };

    let san: Vec<String> = domains
        .iter()
        .enumerate()
        .map(|(i, d)| format!("DNS.{} = {d}", i + 1))
        .chain(
            ips.iter()
                .enumerate()
                .map(|(i, ip)| format!("IP.{} = {ip}", i + 1)),
        )
        .collect();

    if !san.is_empty() {
        lines.push("subjectAltName = @alt_names".into());
        lines.push(String::new());
        lines.push("[alt_names]".into());
        lines.extend(san);
    }

    lines.join("\n") + "\n"
}

/// Subject for a leaf certificate.
pub fn subject(name: &str, domains: &[String], cert_type: CertType) -> String {
    let cn = domains.first().map(String::as_str).unwrap_or(name);
    format!("/CN={cn}/O=cert-gen/OU={}", cert_type.ou())
}

/// Export `cert` + `key` (+ CA chain) to a PKCS#12 bundle.
pub fn export_p12(
    store: &Store,
    key: &Path,
    cert: &Path,
    p12: &Path,
    password: &str,
) -> Result<()> {
    openssl(
        &[
            "pkcs12".to_string(),
            "-export".into(),
            "-out".into(),
            arg(p12),
            "-inkey".into(),
            arg(key),
            "-in".into(),
            arg(cert),
            "-certfile".into(),
            arg(&store.ca_cert),
            "-passout".into(),
            format!("pass:{password}"),
        ],
        true,
    )?;
    Ok(())
}

/// Create a new certificate signed by the CA.
pub fn create_certificate(store: &Store, spec: &CertSpec) -> Result<()> {
    store.ensure_dir()?;

    if !store.ca_exists() {
        println!("CA does not exist. Creating one first...");
        create_ca(store, 3650, 4096)?;
    }

    let name = spec.name;
    let key_path = store.file(name, "key");
    let csr_path = store.file(name, "csr");
    let cert_path = store.file(name, "pub");
    let ext_path = store.file(name, "ext");
    let p12_path = store.file(name, "p12");

    println!("Creating {} certificate: {name}", spec.cert_type.as_str());

    openssl(
        &[
            "genrsa".to_string(),
            "-out".into(),
            arg(&key_path),
            spec.key_size.to_string(),
        ],
        true,
    )?;
    chmod(&key_path, 0o600)?;

    openssl(
        &[
            "req".to_string(),
            "-new".into(),
            "-key".into(),
            arg(&key_path),
            "-out".into(),
            arg(&csr_path),
            "-subj".into(),
            subject(name, spec.domains, spec.cert_type),
        ],
        true,
    )?;

    fs::write(
        &ext_path,
        ext_content(spec.cert_type, spec.domains, spec.ips),
    )
    .with_context(|| format!("writing {}", ext_path.display()))?;

    openssl(
        &[
            "x509".to_string(),
            "-req".into(),
            "-in".into(),
            arg(&csr_path),
            "-CA".into(),
            arg(&store.ca_cert),
            "-CAkey".into(),
            arg(&store.ca_key),
            "-CAserial".into(),
            arg(&store.ca_serial),
            "-CAcreateserial".into(),
            "-out".into(),
            arg(&cert_path),
            "-days".into(),
            spec.days.to_string(),
            "-sha256".into(),
            "-extfile".into(),
            arg(&ext_path),
        ],
        true,
    )?;

    fs::remove_file(&csr_path).with_context(|| format!("removing {}", csr_path.display()))?;
    fs::remove_file(&ext_path).with_context(|| format!("removing {}", ext_path.display()))?;

    println!("✓ Private key: {}", key_path.display());
    println!("✓ Certificate: {}", cert_path.display());

    if spec.create_p12 {
        export_p12(store, &key_path, &cert_path, &p12_path, spec.p12_password)?;
        println!(
            "✓ PKCS#12 bundle: {} (password: {})",
            p12_path.display(),
            spec.p12_password
        );
    }

    println!("\n--- Usage Instructions ---");
    match spec.cert_type {
        CertType::Server => {
            println!("\nNginx configuration:");
            println!("  ssl_certificate {};", cert_path.display());
            println!("  ssl_certificate_key {};", key_path.display());
        }
        CertType::Client => {
            println!("\nClient certificate usage:");
            if spec.create_p12 {
                println!(
                    "  Import {} into your browser/application",
                    p12_path.display()
                );
            }
            println!(
                "  curl --cert {} --key {} https://...",
                cert_path.display(),
                key_path.display()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn server_ext_with_sans() {
        let got = ext_content(
            CertType::Server,
            &s(&["localhost", "app.local"]),
            &s(&["127.0.0.1"]),
        );
        assert_eq!(
            got,
            "basicConstraints = CA:FALSE\n\
             keyUsage = critical,digitalSignature,keyEncipherment\n\
             extendedKeyUsage = serverAuth\n\
             subjectAltName = @alt_names\n\
             \n\
             [alt_names]\n\
             DNS.1 = localhost\n\
             DNS.2 = app.local\n\
             IP.1 = 127.0.0.1\n"
        );
    }

    #[test]
    fn client_ext_without_sans() {
        let got = ext_content(CertType::Client, &[], &[]);
        assert_eq!(
            got,
            "basicConstraints = CA:FALSE\n\
             keyUsage = critical,digitalSignature\n\
             extendedKeyUsage = clientAuth\n"
        );
    }

    #[test]
    fn subject_uses_first_domain_or_name() {
        assert_eq!(
            subject("n", &s(&["a.com", "b.com"]), CertType::Server),
            "/CN=a.com/O=cert-gen/OU=Server"
        );
        assert_eq!(
            subject("alice", &[], CertType::Client),
            "/CN=alice/O=cert-gen/OU=Client"
        );
    }
}
