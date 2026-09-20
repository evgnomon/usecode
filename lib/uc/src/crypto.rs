//! The `openssl enc -aes-256-cbc` container, reimplemented in Rust.
//!
//! Files are byte-for-byte compatible with
//! `openssl enc -aes-256-cbc -pbkdf2 -salt -a`: an 8-byte salt behind a
//! `Salted__` magic, AES-256-CBC with PKCS#7 padding, and base64 wrapped at 64
//! columns. Files written by OpenSSL before 3.0 used an MD5 key derivation
//! instead, so decryption falls back to it.

use crate::{Error, Result};
use aes::Aes256;
use aes::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest, Md5};
use sha2::Sha256;
use zeroize::Zeroize;

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;

const MAGIC: &[u8; 8] = b"Salted__";
const SALT_LEN: usize = 8;
const KEY_LEN: usize = 32;
const IV_LEN: usize = 16;
const BLOCK_LEN: usize = 16;
/// The iteration count `openssl enc -pbkdf2` uses when `-iter` is not given.
const PBKDF2_ROUNDS: u32 = 10_000;
/// Columns per line of base64, matching `openssl enc -a`.
const B64_WIDTH: usize = 64;

/// Which key derivation opened a file, so the caller can nudge towards a
/// re-encrypt when it was the legacy one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kdf {
    Pbkdf2,
    LegacyMd5,
}

/// A derived AES key and IV, wiped when dropped.
struct KeyMaterial {
    key: [u8; KEY_LEN],
    iv: [u8; IV_LEN],
}

impl Drop for KeyMaterial {
    fn drop(&mut self) {
        self.key.zeroize();
        self.iv.zeroize();
    }
}

fn derive_pbkdf2(password: &[u8], salt: &[u8]) -> KeyMaterial {
    let mut out = [0u8; KEY_LEN + IV_LEN];
    pbkdf2::pbkdf2_hmac::<Sha256>(password, salt, PBKDF2_ROUNDS, &mut out);
    let material = KeyMaterial {
        key: out[..KEY_LEN].try_into().expect("key slice is 32 bytes"),
        iv: out[KEY_LEN..].try_into().expect("iv slice is 16 bytes"),
    };
    out.zeroize();
    material
}

/// OpenSSL's pre-3.0 `EVP_BytesToKey` with MD5 and a single iteration.
fn derive_legacy_md5(password: &[u8], salt: &[u8]) -> KeyMaterial {
    let mut out = Vec::with_capacity(KEY_LEN + IV_LEN);
    let mut previous: Vec<u8> = Vec::new();
    while out.len() < KEY_LEN + IV_LEN {
        let mut hasher = Md5::new();
        hasher.update(&previous);
        hasher.update(password);
        hasher.update(salt);
        previous = hasher.finalize().to_vec();
        out.extend_from_slice(&previous);
    }
    let material = KeyMaterial {
        key: out[..KEY_LEN].try_into().expect("key slice is 32 bytes"),
        iv: out[KEY_LEN..KEY_LEN + IV_LEN]
            .try_into()
            .expect("iv slice is 16 bytes"),
    };
    previous.zeroize();
    out.zeroize();
    material
}

fn random_salt() -> Result<[u8; SALT_LEN]> {
    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt).map_err(|err| {
        Error::Io(
            "reading random bytes for the salt".to_string(),
            std::io::Error::other(err.to_string()),
        )
    })?;
    Ok(salt)
}

/// Encrypts `plaintext` into the raw (not yet base64) `Salted__` container.
pub fn encrypt_raw(plaintext: &[u8], password: &[u8]) -> Result<Vec<u8>> {
    let salt = random_salt()?;
    let material = derive_pbkdf2(password, &salt);

    let mut buf = vec![0u8; plaintext.len() + BLOCK_LEN];
    buf[..plaintext.len()].copy_from_slice(plaintext);
    let ciphertext_len = Aes256CbcEnc::new(&material.key.into(), &material.iv.into())
        .encrypt_padded::<Pkcs7>(&mut buf, plaintext.len())
        .expect("buffer holds the plaintext plus a full padding block")
        .len();
    buf.truncate(ciphertext_len);

    let mut out = Vec::with_capacity(MAGIC.len() + SALT_LEN + ciphertext_len);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.append(&mut buf);
    Ok(out)
}

/// Decrypts a raw `Salted__` container, trying the current key derivation and
/// then the legacy one.
pub fn decrypt_raw(raw: &[u8], password: &[u8]) -> Result<(Vec<u8>, Kdf)> {
    if raw.len() < MAGIC.len() + SALT_LEN || &raw[..MAGIC.len()] != MAGIC {
        return Err(Error::Format(
            "missing the 'Salted__' header; the file was not produced by uc-encrypt".to_string(),
        ));
    }
    let salt = &raw[MAGIC.len()..MAGIC.len() + SALT_LEN];
    let ciphertext = &raw[MAGIC.len() + SALT_LEN..];
    if ciphertext.is_empty() || !ciphertext.len().is_multiple_of(BLOCK_LEN) {
        return Err(Error::Format(
            "ciphertext length is not a whole number of AES blocks (truncated or corrupted?)"
                .to_string(),
        ));
    }

    for kdf in [Kdf::Pbkdf2, Kdf::LegacyMd5] {
        let material = match kdf {
            Kdf::Pbkdf2 => derive_pbkdf2(password, salt),
            Kdf::LegacyMd5 => derive_legacy_md5(password, salt),
        };
        let mut buf = ciphertext.to_vec();
        let decrypted = Aes256CbcDec::new(&material.key.into(), &material.iv.into())
            .decrypt_padded::<Pkcs7>(&mut buf)
            .map(<[u8]>::to_vec);
        buf.zeroize();
        if let Ok(plaintext) = decrypted {
            return Ok((plaintext, kdf));
        }
    }
    Err(Error::Decrypt)
}

/// Base64 encodes `raw` the way `openssl enc -a` does: 64 columns, newline
/// terminated.
pub fn armor(raw: &[u8]) -> String {
    let encoded = STANDARD.encode(raw);
    let mut out = String::with_capacity(encoded.len() + encoded.len() / B64_WIDTH + 1);
    for chunk in encoded.as_bytes().chunks(B64_WIDTH) {
        out.push_str(std::str::from_utf8(chunk).expect("base64 is ASCII"));
        out.push('\n');
    }
    out
}

/// Decodes armored input, ignoring whitespace so a stray newline or a CRLF
/// round trip cannot break the decode.
pub fn dearmor(armored: &[u8]) -> Result<Vec<u8>> {
    let compact: Vec<u8> = armored
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if compact.is_empty() {
        return Err(Error::Format("file is empty".to_string()));
    }
    STANDARD
        .decode(&compact)
        .map_err(|err| Error::Format(format!("not valid base64 ({err})")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_armor() {
        let plaintext = b"attack at dawn\n".repeat(40);
        let raw = encrypt_raw(&plaintext, b"hunter2").unwrap();
        let armored = armor(&raw);
        let (decrypted, kdf) =
            decrypt_raw(&dearmor(armored.as_bytes()).unwrap(), b"hunter2").unwrap();
        assert_eq!(decrypted, plaintext);
        assert_eq!(kdf, Kdf::Pbkdf2);
    }

    #[test]
    fn round_trips_an_empty_file() {
        let raw = encrypt_raw(b"", b"hunter2").unwrap();
        let (decrypted, _) = decrypt_raw(&raw, b"hunter2").unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn rejects_the_wrong_password() {
        let raw = encrypt_raw(b"attack at dawn\n", b"hunter2").unwrap();
        assert!(matches!(
            decrypt_raw(&raw, b"hunter3"),
            Err(Error::Decrypt) | Err(Error::Format(_))
        ));
    }

    #[test]
    fn rejects_a_file_without_the_header() {
        assert!(matches!(
            decrypt_raw(b"not a boom file at all, really", b"hunter2"),
            Err(Error::Format(_))
        ));
    }

    #[test]
    fn armor_wraps_at_64_columns() {
        let armored = armor(&[0u8; 200]);
        let mut lines = armored.lines();
        assert_eq!(lines.next().unwrap().len(), 64);
        assert!(armored.ends_with('\n'));
    }

    #[test]
    fn dearmor_ignores_whitespace() {
        let raw = encrypt_raw(b"hello\n", b"hunter2").unwrap();
        let noisy = format!("  {}\r\n\r\n", armor(&raw).replace('\n', "\r\n"));
        assert_eq!(dearmor(noisy.as_bytes()).unwrap(), raw);
    }

    /// The legacy branch is what lets files written by OpenSSL 1.x still open.
    #[test]
    fn opens_a_legacy_md5_container() {
        let salt = [0x41u8; SALT_LEN];
        let material = derive_legacy_md5(b"hunter2", &salt);
        let plaintext = b"legacy secret\n";
        let mut buf = vec![0u8; plaintext.len() + BLOCK_LEN];
        buf[..plaintext.len()].copy_from_slice(plaintext);
        let len = Aes256CbcEnc::new(&material.key.into(), &material.iv.into())
            .encrypt_padded::<Pkcs7>(&mut buf, plaintext.len())
            .unwrap()
            .len();
        let mut raw = Vec::new();
        raw.extend_from_slice(MAGIC);
        raw.extend_from_slice(&salt);
        raw.extend_from_slice(&buf[..len]);

        let (decrypted, kdf) = decrypt_raw(&raw, b"hunter2").unwrap();
        assert_eq!(decrypted, plaintext);
        assert_eq!(kdf, Kdf::LegacyMd5);
    }
}
