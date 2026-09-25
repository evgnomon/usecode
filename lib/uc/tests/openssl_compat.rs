// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Proves the container stays interchangeable with the `openssl enc` command
//! the shell implementation used, in both directions.

use std::io::Write;
use std::process::{Command, Stdio};
use uc::crypto::{self, Kdf};

const PASSWORD: &str = "correct horse battery staple";

fn openssl_available() -> bool {
    Command::new("openssl")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Runs `openssl enc` with the given arguments, feeding `input` on stdin.
fn openssl_enc(args: &[&str], input: &[u8]) -> Option<Vec<u8>> {
    let mut child = Command::new("openssl")
        .arg("enc")
        .args(args)
        .arg("-pass")
        .arg(format!("pass:{PASSWORD}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(input).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status.success().then_some(out.stdout)
}

#[test]
fn openssl_decrypts_what_uc_encrypt_writes() {
    if !openssl_available() {
        eprintln!("skipping: openssl not installed");
        return;
    }
    let plaintext = b"the launch codes are in the other file\n".repeat(10);
    let armored = crypto::armor(&crypto::encrypt_raw(&plaintext, PASSWORD.as_bytes()).unwrap());

    let decrypted = openssl_enc(&["-d", "-aes-256-cbc", "-pbkdf2", "-a"], armored.as_bytes())
        .expect("openssl failed to decrypt a uc-encrypt file");
    assert_eq!(decrypted, plaintext);
}

#[test]
fn uc_decrypt_reads_what_openssl_writes() {
    if !openssl_available() {
        eprintln!("skipping: openssl not installed");
        return;
    }
    let plaintext = b"the launch codes are in the other file\n".repeat(10);
    let armored = openssl_enc(&["-aes-256-cbc", "-pbkdf2", "-salt", "-a"], &plaintext)
        .expect("openssl failed to encrypt");

    let (decrypted, kdf) =
        crypto::decrypt_raw(&crypto::dearmor(&armored).unwrap(), PASSWORD.as_bytes()).unwrap();
    assert_eq!(decrypted, plaintext);
    assert_eq!(kdf, Kdf::Pbkdf2);
}

/// Files written by the pre-3.0 OpenSSL default must still open.
#[test]
fn uc_decrypt_reads_a_legacy_openssl_file() {
    if !openssl_available() {
        eprintln!("skipping: openssl not installed");
        return;
    }
    let plaintext = b"an old secret\n";
    let Some(armored) = openssl_enc(&["-aes-256-cbc", "-md", "md5", "-salt", "-a"], plaintext)
    else {
        eprintln!("skipping: this openssl refuses the legacy -md md5 KDF");
        return;
    };

    let (decrypted, kdf) =
        crypto::decrypt_raw(&crypto::dearmor(&armored).unwrap(), PASSWORD.as_bytes()).unwrap();
    assert_eq!(decrypted, plaintext);
    assert_eq!(kdf, Kdf::LegacyMd5);
}
