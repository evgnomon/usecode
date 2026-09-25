// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

mod swanctl;

use std::fs;
use std::path::Path;
use std::process::{self, Command, Stdio};

use clap::Parser;

use crate::swanctl::{Local, Peer};

/// Full-mesh IPsec VPN using certificate authentication (PKI/pubkey) – no shared PSK.
///
/// For 4 hosts (alpha ↔ beta ↔ gamma ↔ delta), run on each host with its own cert/key.
///
/// Example usage on alpha:
///
///   sudo ipmesh \
///     --local-ip 203.0.113.47 \
///     --local-id @alpha \
///     --virtual-ip 192.168.88.10/32 \
///     --peer beta 198.51.100.215 @beta \
///     --peer gamma 192.0.2.178 @gamma \
///     --peer delta 203.0.113.189 @delta \
///     --cert-file ./alphaCert.pem \
///     --key-file ./alphaKey.pem \
///     --ca-file ./caCert.pem
///
/// Certificate generation (run once on secure machine):
///
///   ipsec pki --gen --type rsa --size 4096 --outform pem > caKey.pem
///   ipsec pki --self --ca --lifetime 3650 ... > caCert.pem
///   # Then generate per-host keys/certs signed by CA
#[derive(Parser)]
#[command(name = "ipmesh", verbatim_doc_comment)]
struct Cli {
    /// This host's public IPv4/IPv6 address
    #[arg(long)]
    local_ip: String,
    /// This host's IKE identity e.g. @alpha
    #[arg(long)]
    local_id: String,
    /// This host's virtual IP e.g. 192.168.88.10/32
    #[arg(long)]
    virtual_ip: String,
    /// Peer: <name> <public-ip> <@id>   (repeat for each peer)
    #[arg(long, num_args = 3, value_names = ["NAME", "PUBLIC_IP", "ID"])]
    peer: Vec<String>,
    /// Path to this host's certificate (e.g. alphaCert.pem)
    #[arg(long)]
    cert_file: String,
    /// Path to this host's private key (e.g. alphaKey.pem)
    #[arg(long)]
    key_file: String,
    /// Path to CA certificate (caCert.pem)
    #[arg(long)]
    ca_file: String,
    /// Install strongSwan if not found
    #[arg(long)]
    install: bool,
}

const PRIV_DIR: &str = "/etc/swanctl/private/";
const X509_DIR: &str = "/etc/swanctl/x509/";
const CA_DIR: &str = "/etc/swanctl/x509ca/";
const CONFIG_PATH: &str = "/etc/swanctl/swanctl.conf";

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("{msg}");
    process::exit(1)
}

/// Run `sudo <cmd>`; exit 1 with the command and its stderr on failure.
/// When `capture` is false the output goes straight to the terminal.
fn sudo(cmd: &[&str], capture: bool) {
    let mut full = vec!["sudo"];
    full.extend_from_slice(cmd);
    let mut c = Command::new(full[0]);
    c.args(&full[1..]);
    let (stdout, stderr) = match capture {
        true => (Stdio::piped(), Stdio::piped()),
        false => (Stdio::inherit(), Stdio::inherit()),
    };
    let out = c
        .stdin(Stdio::inherit())
        .stdout(stdout)
        .stderr(stderr)
        .output()
        .unwrap_or_else(|e| die(format!("Command failed: {}\n{e}", full.join(" "))));
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = match err.is_empty() {
            true => "No error output".into(),
            false => err,
        };
        die(format!("Command failed: {}\n{err}", full.join(" ")));
    }
}

fn which(bin: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|d| {
        let p = d.join(bin);
        p.is_file() && is_executable(&p)
    })
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

fn install_strongswan_if_missing() {
    if which("swanctl") {
        println!("strongSwan already installed.");
        return;
    }
    println!("Installing strongSwan...");
    sudo(&["apt", "update"], false);
    sudo(
        &["apt", "install", "-y", "strongswan", "strongswan-swanctl"],
        false,
    );
    println!("strongSwan installed.");
}

fn basename(p: &str) -> String {
    // Matches os.path.basename: everything after the last '/'.
    p.rsplit('/').next().unwrap_or(p).to_string()
}

fn copy_credentials(key: &str, cert: &str, ca: &str) {
    for d in [PRIV_DIR, X509_DIR, CA_DIR] {
        sudo(&["mkdir", "-p", d], true);
    }
    let key_dest = format!("{PRIV_DIR}{}", basename(key));
    let cert_dest = format!("{X509_DIR}{}", basename(cert));
    let ca_dest = format!("{CA_DIR}{}", basename(ca));

    sudo(&["cp", key, &key_dest], true);
    sudo(&["cp", cert, &cert_dest], true);
    sudo(&["cp", ca, &ca_dest], true);

    sudo(&["chmod", "600", &key_dest], true);
    sudo(
        &["chown", "root:root", &key_dest, &cert_dest, &ca_dest],
        true,
    );

    println!("Credentials installed:");
    println!("  Key → {key_dest}");
    println!("  Cert → {cert_dest}");
    println!("  CA → {ca_dest}");
}

fn write_config(content: &str) {
    // Private temp file (O_EXCL, 0600) instead of a fixed /tmp path, which a
    // local user could pre-create or symlink.
    let tmp = tempfile::Builder::new()
        .prefix("swanctl.conf.")
        .tempfile()
        .unwrap_or_else(|e| die(format!("creating temp file: {e}")));
    fs::write(tmp.path(), content)
        .unwrap_or_else(|e| die(format!("{}: {e}", tmp.path().display())));
    let tmp = tmp.into_temp_path();
    let tmp_str = tmp.to_string_lossy();
    sudo(&["mv", &tmp_str, CONFIG_PATH], true);
    println!("swanctl.conf generated → {CONFIG_PATH}");
}

fn load_configuration() {
    for flag in [
        "--load-creds",
        "--load-conns",
        "--load-authorities",
        "--load-all",
    ] {
        sudo(&["swanctl", flag], true);
    }
    println!("Configuration loaded.");
}

fn main() {
    let cli = Cli::parse();

    // SAFETY: geteuid has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } != 0 {
        die("This script must be run as root (sudo).");
    }

    if cli.install {
        install_strongswan_if_missing();
    }

    if cli.peer.is_empty() {
        die("At least one --peer required.");
    }
    let peers: Vec<Peer> = cli
        .peer
        .as_chunks::<3>()
        .0
        .iter()
        .map(|[name, ip, id]| Peer {
            name: name.clone(),
            ip: ip.clone(),
            id: id.clone(),
        })
        .collect();

    let cert = basename(&cli.cert_file);
    let key = basename(&cli.key_file);
    let ca = basename(&cli.ca_file);

    copy_credentials(&cli.key_file, &cli.cert_file, &cli.ca_file);
    let local = Local {
        ip: &cli.local_ip,
        id: &cli.local_id,
        virtual_ip: &cli.virtual_ip,
        cert: &cert,
        key: &key,
        ca: &ca,
    };
    write_config(&swanctl::render(&local, &peers));
    load_configuration();

    println!("\nSetup complete. Verify:");
    println!("  sudo swanctl --list-certs --all");
    println!("  sudo swanctl --list-sas");
    println!("  sudo swanctl --list-conns");
    println!("  ip addr show | grep 192.168.88");
    println!("  ping 192.168.88.11   # test connectivity to another host's virtual IP");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basename_like_python() {
        assert_eq!(basename("./a/b.pem"), "b.pem");
        assert_eq!(basename("b.pem"), "b.pem");
        assert_eq!(basename("dir/"), "");
    }

    #[test]
    fn peers_parse_in_triples() {
        let cli = Cli::try_parse_from([
            "ipmesh",
            "--local-ip",
            "1",
            "--local-id",
            "@a",
            "--virtual-ip",
            "v",
            "--peer",
            "b",
            "2",
            "@b",
            "--peer",
            "c",
            "3",
            "@c",
            "--cert-file",
            "c",
            "--key-file",
            "k",
            "--ca-file",
            "ca",
        ])
        .unwrap();
        assert_eq!(cli.peer, ["b", "2", "@b", "c", "3", "@c"]);
    }
}
