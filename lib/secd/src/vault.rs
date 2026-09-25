// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! "Ansible Vault"-style encryption: PBKDF2-HMAC-SHA256 (10000 rounds, 80
//! bytes) → AES-256-CTR key, HMAC-SHA256 key and IV; PKCS#7 padded plaintext.
//!
//! The on-disk layout is the one written by the original secd tool:
//! `$ANSIBLE_VAULT;1.1;AES256` followed by the upper-case hex of
//! `salt(32) || hmac(32) || ciphertext`, wrapped at 80 columns.

use aes::Aes256;
use ctr::cipher::{KeyIvInit, StreamCipher};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type Aes256Ctr = ctr::Ctr128BE<Aes256>;
type HmacSha256 = Hmac<Sha256>;

pub const HEADER: &str = "$ANSIBLE_VAULT;1.1;AES256";
const ITERATIONS: u32 = 10000;
const BLOCK: usize = 16;

struct Keys {
    key: [u8; 32],
    hmac_key: [u8; 32],
    iv: [u8; 16],
}

fn derive(password: &str, salt: &[u8]) -> Keys {
    let mut out = [0u8; 80];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, ITERATIONS, &mut out);
    let mut k = Keys {
        key: [0; 32],
        hmac_key: [0; 32],
        iv: [0; 16],
    };
    k.key.copy_from_slice(&out[..32]);
    k.hmac_key.copy_from_slice(&out[32..64]);
    k.iv.copy_from_slice(&out[64..]);
    k
}

fn mac(key: &[u8], data: &[u8]) -> HmacSha256 {
    let mut h = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC accepts any key length");
    h.update(data);
    h
}

/// Encrypt `plaintext` with a fresh random salt.
pub fn encrypt(plaintext: &str, password: &str) -> String {
    let mut salt = [0u8; 32];
    getrandom::fill(&mut salt).expect("OS random number generator");
    encrypt_with_salt(plaintext, password, &salt)
}

fn encrypt_with_salt(plaintext: &str, password: &str, salt: &[u8; 32]) -> String {
    let k = derive(password, salt);

    let mut data = plaintext.as_bytes().to_vec();
    let pad = BLOCK - data.len() % BLOCK;
    data.extend(std::iter::repeat_n(pad as u8, pad));

    Aes256Ctr::new(&k.key.into(), &k.iv.into()).apply_keystream(&mut data);
    let tag = mac(&k.hmac_key, &data).finalize().into_bytes();

    let mut payload = Vec::with_capacity(64 + data.len());
    payload.extend_from_slice(salt);
    payload.extend_from_slice(&tag);
    payload.extend_from_slice(&data);
    let hexed = hex::encode_upper(payload);

    let lines: Vec<&str> = hexed
        .as_bytes()
        .chunks(80)
        .map(|c| std::str::from_utf8(c).expect("hex is ASCII"))
        .collect();
    format!("{HEADER}\n{}\n", lines.join("\n"))
}

/// Decrypt a vault document. Errors carry user-facing messages.
pub fn decrypt(vault_text: &str, password: &str) -> Result<String, String> {
    let mut lines = vault_text.lines();
    if lines.next() != Some(HEADER) {
        return Err("Invalid vault header".into());
    }
    let hex_payload: String = lines.collect();
    if !hex_payload.len().is_multiple_of(2) {
        return Err("Odd-length string".into());
    }
    let payload = hex::decode(&hex_payload).map_err(|_| "Non-hexadecimal digit found")?;

    let salt = &payload[..payload.len().min(32)];
    let hmac_val = &payload[payload.len().min(32)..payload.len().min(64)];
    let ciphertext = &payload[payload.len().min(64)..];

    let k = derive(password, salt);
    mac(&k.hmac_key, ciphertext)
        .verify_slice(hmac_val)
        .map_err(|_| "HMAC verification failed – wrong password?")?;

    let mut data = ciphertext.to_vec();
    Aes256Ctr::new(&k.key.into(), &k.iv.into()).apply_keystream(&mut data);

    let pad = data.last().copied().unwrap_or(0) as usize;
    let valid = data.len().is_multiple_of(BLOCK)
        && (1..=BLOCK).contains(&pad)
        && data[data.len() - pad..].iter().all(|&b| b as usize == pad);
    if !valid {
        return Err("Invalid padding bytes.".into());
    }
    data.truncate(data.len() - pad);

    String::from_utf8(data).map_err(|e| format!("'utf-8' codec can't decode bytes: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for text in ["", "s3cret", "exactly16bytes!!", &"x".repeat(200)] {
            let v = encrypt(text, "pw");
            assert!(v.starts_with("$ANSIBLE_VAULT;1.1;AES256\n"));
            assert!(v.ends_with('\n'));
            assert!(v.lines().skip(1).all(|l| l.len() <= 80));
            assert_eq!(decrypt(&v, "pw").unwrap(), text);
        }
    }

    #[test]
    fn wrong_password() {
        let v = encrypt("s3cret", "pw");
        assert_eq!(
            decrypt(&v, "nope").unwrap_err(),
            "HMAC verification failed – wrong password?"
        );
    }

    #[test]
    fn bad_header() {
        assert_eq!(
            decrypt("junk\nAA", "pw").unwrap_err(),
            "Invalid vault header"
        );
        assert_eq!(decrypt("", "pw").unwrap_err(), "Invalid vault header");
    }

    #[test]
    fn deterministic_layout() {
        let salt = [7u8; 32];
        let v = encrypt_with_salt("hello", "pw", &salt);
        let body: String = v.lines().skip(1).collect();
        // salt + hmac + one AES block
        assert_eq!(body.len(), 2 * (32 + 32 + 16));
        assert!(body.starts_with(&"07".repeat(32)));
        assert_eq!(body, body.to_uppercase());
    }
}
