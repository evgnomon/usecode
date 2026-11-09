// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Bundle a zygote function key/cert (plus CA) into a PKCS#12 file.

use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, exit};

fn openssl_args(home: &str, key: &str) -> Vec<String> {
    let key_path = format!("{home}/.config/zygote/certs/functions/{key}");
    vec![
        "pkcs12".into(),
        "-export".into(),
        "-out".into(),
        format!("{key_path}/{key}_cert.p12"),
        "-inkey".into(),
        format!("{key_path}/{key}_key.pem"),
        "-in".into(),
        format!("{key_path}/{key}_cert.pem"),
        "-certfile".into(),
        format!("{home}/.config/zygote/certs/ca/ca_cert.pem"),
    ]
}

const USAGE: &str = "usage: uc secret cert p12 KEYNAME

Bundles the zygote function key and certificate KEYNAME (e.g. user_1.zygote)
with the zygote CA into a PKCS#12 file, with openssl:
  ~/.config/zygote/certs/functions/KEYNAME/KEYNAME_cert.p12
from KEYNAME_key.pem and KEYNAME_cert.pem there and
~/.config/zygote/certs/ca/ca_cert.pem. openssl asks for the export password.";

fn main() {
    let args: Vec<String> = env::args().collect();
    if args[1..].iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        exit(0);
    }
    if args.len() != 2 {
        eprintln!("{USAGE}");
        exit(1);
    }
    let home = env::var("HOME").unwrap_or_default();
    let err = Command::new("openssl")
        .args(openssl_args(&home, &args[1]))
        .exec();
    eprintln!("uc-secret-cert-p12-bundle: openssl: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}

#[cfg(test)]
mod tests {
    use super::openssl_args;

    #[test]
    fn builds_paths() {
        let a = openssl_args("/h", "k");
        assert_eq!(a[3], "/h/.config/zygote/certs/functions/k/k_cert.p12");
        assert_eq!(a[5], "/h/.config/zygote/certs/functions/k/k_key.pem");
        assert_eq!(a[7], "/h/.config/zygote/certs/functions/k/k_cert.pem");
        assert_eq!(a[9], "/h/.config/zygote/certs/ca/ca_cert.pem");
    }
}
