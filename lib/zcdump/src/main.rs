// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Fetch the zygote CA certificate (into ./ca_cert.pem) and the user_1/user_2
//! function certificates from controller.zygote, bundle each user's cert and
//! key into PKCS#12 with `mkp12`, then delete the plaintext key.

use std::env;
use std::fs::{self, File};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, exit};

const CERT_DIR: &str = "/var/lib/zygote/certs";

fn code(r: std::io::Result<std::process::ExitStatus>, prog: &str) -> i32 {
    match r {
        Ok(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
        Err(e) => {
            eprintln!("zcdump: {prog}: {e}");
            if e.kind() == std::io::ErrorKind::NotFound {
                127
            } else {
                126
            }
        }
    }
}

/// `ssh controller.zygote -t sudo cat <remote> > <local>`
fn fetch(remote: &str, local: &str) -> i32 {
    let out = match File::create(local) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("zcdump: {local}: {e}");
            return 1;
        }
    };
    let r = Command::new("ssh")
        .args(["controller.zygote", "-t", "sudo", "cat", remote])
        .stdout(out)
        .status();
    code(r, "ssh")
}

fn main() {
    let home = env::var("HOME").unwrap_or_default();
    fetch(&format!("{CERT_DIR}/ca/ca_cert.pem"), "ca_cert.pem");

    let mut rc = 0;
    for i in 1..=2 {
        let user = format!("user_{i}.zygote");
        let local = format!("{home}/.config/zygote/certs/functions/{user}");
        let remote = format!("{CERT_DIR}/functions/{user}");
        if let Err(e) = fs::create_dir_all(&local) {
            eprintln!("mkdir: cannot create directory '{local}': {e}");
        }
        let key = format!("{local}/{user}_key.pem");
        fetch(
            &format!("{remote}/{user}_cert.pem"),
            &format!("{local}/{user}_cert.pem"),
        );
        fetch(&format!("{remote}/{user}_key.pem"), &key);
        code(Command::new("mkp12").arg(&user).status(), "mkp12");
        rc = match fs::remove_file(&key) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("rm: cannot remove '{key}': {e}");
                1
            }
        };
    }
    exit(rc);
}
