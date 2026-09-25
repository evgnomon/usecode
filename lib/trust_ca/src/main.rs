// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

mod add;
mod list;
mod pem;
mod remove;
mod sh;

use std::process::exit;

const USAGE: &str = "\
Usage: trust_ca <host:port>           Fetch and install the CA serving this endpoint
       trust_ca --remove <host:port>  Remove the CA installed for this endpoint
       trust_ca --list [host[:port]]  List custom-trusted CAs (optionally filtered)
       trust_ca -h|--help             Show this help

What it does:
  Connects to <host:port> via TLS, extracts the CA cert(s) from the chain
  (skipping the leaf), and installs them into three trust stores:
    - /usr/local/share/ca-certificates/   (system-wide; used by curl, Go, openssl)
    - /etc/containers/certs.d/<host:port>/ca.crt   (podman/skopeo, per-registry)
    - ~/.pki/nssdb                        (Brave/Chrome, via certutil)

Examples:
  trust_ca cr.main.sys.local.zygote.run:443
  trust_ca --remove cr.main.sys.local.zygote.run:443
  trust_ca --list                  # all custom-trusted CAs across stores
  trust_ca --list example.com      # only CAs whose host matches \"example.com\"

Notes:
  - The endpoint must be live — the cert is scraped from the TLS handshake.
  - Restart Brave/Chrome after add/remove for browser changes to take effect.
  - Requires sudo for the system and container trust stores.";

pub const SYSTEM_DIR: &str = "/usr/local/share/ca-certificates";
pub const CONTAINERS_DIR: &str = "/etc/containers/certs.d";

enum Action {
    Add,
    Remove,
    List,
}

/// Host part of `host:port` (everything before the first ':').
pub fn host_of(registry: &str) -> &str {
    registry.split(':').next().unwrap_or("")
}

/// Name used for the system store file and NSS nicknames.
pub fn cert_name(host: &str) -> String {
    host.replace('.', "-")
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let action = match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            println!("{USAGE}");
            exit(0);
        }
        Some("--remove") => Action::Remove,
        Some("--list") => Action::List,
        _ => Action::Add,
    };
    if !matches!(action, Action::Add) {
        args.remove(0);
    }

    if let Action::List = action {
        let filter = args.first().map(|r| host_of(r)).unwrap_or("");
        list::list_certs(filter);
        exit(0);
    }

    if args.len() != 1 {
        println!("{USAGE}");
        exit(1);
    }
    let registry = &args[0];
    let code = match action {
        Action::Remove => remove::remove(registry),
        _ => add::add(registry),
    };
    exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_and_name() {
        assert_eq!(host_of("cr.main.sys:443"), "cr.main.sys");
        assert_eq!(host_of("example.com"), "example.com");
        assert_eq!(host_of("a:1:2"), "a");
        assert_eq!(cert_name("cr.main.sys"), "cr-main-sys");
    }
}
