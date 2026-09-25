// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `trust_ca --remove <host:port>`.

use crate::pem::nickname;
use crate::sh::{capture, has_cmd, run};
use crate::{CONTAINERS_DIR, SYSTEM_DIR, cert_name, host_of};

/// NSS nicknames belonging to `name`: exactly `name` or `name-*`.
pub fn owned_nicknames(listing: &str, name: &str) -> Vec<String> {
    let prefix = format!("{name}-");
    listing
        .lines()
        .skip(4)
        .filter(|l| !l.is_empty())
        .map(nickname)
        .filter(|n| !n.is_empty() && (n == name || n.starts_with(&prefix)))
        .collect()
}

pub fn remove(registry: &str) -> i32 {
    match try_remove(registry) {
        Ok(()) => 0,
        Err(code) => code,
    }
}

fn try_remove(registry: &str) -> Result<(), i32> {
    let name = cert_name(host_of(registry));
    println!("Removing certificates for {registry} ...");

    println!();
    println!("==> Removing from system CA trust store ...");
    run("sudo", &["rm", "-f", &format!("{SYSTEM_DIR}/{name}.crt")])?;
    run("sudo", &["update-ca-certificates"])?;

    println!();
    println!("==> Removing from container runtime trust store ...");
    run(
        "sudo",
        &["rm", "-rf", &format!("{CONTAINERS_DIR}/{registry}")],
    )?;

    println!();
    if has_cmd("certutil") {
        println!("==> Removing from NSS database (Brave/Chrome) ...");
        let home = std::env::var("HOME").unwrap_or_default();
        let db = format!("sql:{home}/.pki/nssdb");
        let (_, listing) = capture("certutil", &["-d", &db, "-L"], None);
        for nick in owned_nicknames(&listing, &name) {
            println!("  Removing: {nick}");
            let _ = capture("certutil", &["-d", &db, "-D", "-n", &nick], None);
        }
    } else {
        println!("==> Skipping NSS database (certutil not found)");
    }

    println!();
    println!("Done. Restart Brave/Chrome for browser changes to take effect.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_owned_nicknames() {
        let listing = "\nCertificate Nickname                                         Trust Attributes\n                                                             SSL,S/MIME,JAR/XPI\n\n\
cr-main-0                                                    CT,C,C\n\
cr-main-1                                                    CT,C,C\n\
cr-main                                                      CT,C,C\n\
cr-mainx                                                     CT,C,C\n\
other                                                        CT,C,C\n";
        assert_eq!(
            owned_nicknames(listing, "cr-main"),
            ["cr-main-0", "cr-main-1", "cr-main"]
        );
    }
}
