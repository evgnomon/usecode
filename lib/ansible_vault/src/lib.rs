// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The ansible-vault file format, so a vaulted file can be read and
//! written without ansible installed. What comes out opens with
//! `ansible-vault edit` and the other way round. [`password`] finds the
//! password the way ansible-vault would.
//!
//! `$ANSIBLE_VAULT;1.1;AES256` (or `1.2;AES256;<vault-id>`) is followed by
//! hex, wrapped at 80 columns, of the three lines
//! `hex(salt)\nhex(hmac)\nhex(ciphertext)`. PBKDF2-HMAC-SHA256 (10000
//! rounds) of the password and salt gives 80 bytes: the AES-256-CTR key,
//! the HMAC-SHA256 key and the IV. The plaintext is PKCS#7 padded and the
//! HMAC covers the ciphertext.

use std::fmt;
use std::io::Write;
use std::path::Path;

use aes::Aes256;
use ctr::cipher::{KeyIvInit, StreamCipher};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

pub mod password;

type Aes256Ctr = ctr::Ctr128BE<Aes256>;
type HmacSha256 = Hmac<Sha256>;

/// The header `encrypt` writes. 1.1 carries no vault id, which is what
/// plain `ansible-vault encrypt` writes too.
pub const HEADER: &str = "$ANSIBLE_VAULT;1.1;AES256";
const ITERATIONS: u32 = 10000;
const BLOCK: usize = 16;
const WIDTH: usize = 80;

/// A failure, as a message for people: a wrong password, a malformed
/// file, a password file that wouldn't run.
#[derive(Debug)]
pub struct Error(pub String);

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

macro_rules! bail {
    ($($arg:tt)*) => { return Err($crate::Error(format!($($arg)*))) };
}
pub(crate) use bail;

/// Prefix an error with what was being done, e.g. "decrypt x.yml: ...".
pub(crate) fn ctx<T, E: fmt::Display>(
    r: std::result::Result<T, E>,
    what: impl FnOnce() -> String,
) -> Result<T> {
    r.map_err(|e| Error(format!("{}: {e}", what())))
}

struct Keys {
    key: [u8; 32],
    hmac_key: [u8; 32],
    iv: [u8; 16],
}

fn derive(password: &[u8], salt: &[u8]) -> Keys {
    let mut out = [0u8; 80];
    pbkdf2::pbkdf2_hmac::<Sha256>(password, salt, ITERATIONS, &mut out);
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
    let mut h = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    h.update(data);
    h
}

/// Whether `text` is a vaulted file rather than plaintext.
pub fn is_encrypted(text: &[u8]) -> bool {
    String::from_utf8_lossy(text)
        .trim_start()
        .starts_with("$ANSIBLE_VAULT;")
}

/// Encrypt `plaintext` with a fresh random salt.
pub fn encrypt(plaintext: &[u8], password: &[u8]) -> Result<String> {
    let mut salt = [0u8; 32];
    ctx(getrandom::fill(&mut salt), || "read random salt".into())?;
    Ok(encrypt_with_salt(plaintext, password, &salt))
}

fn encrypt_with_salt(plaintext: &[u8], password: &[u8], salt: &[u8; 32]) -> String {
    let k = derive(password, salt);

    let mut data = plaintext.to_vec();
    let pad = BLOCK - data.len() % BLOCK;
    data.extend(std::iter::repeat_n(pad as u8, pad));

    Aes256Ctr::new(&k.key.into(), &k.iv.into()).apply_keystream(&mut data);
    let tag = mac(&k.hmac_key, &data).finalize().into_bytes();

    let inner = format!(
        "{}\n{}\n{}",
        hex::encode(salt),
        hex::encode(tag),
        hex::encode(&data)
    );
    let outer = hex::encode(inner);

    let mut out = String::from(HEADER);
    for line in outer.as_bytes().chunks(WIDTH) {
        out.push('\n');
        out.push_str(std::str::from_utf8(line).expect("hex is ASCII"));
    }
    out.push('\n');
    out
}

/// Decrypt a vaulted file. A wrong password is reported as such.
pub fn decrypt(vault_text: &str, password: &[u8]) -> Result<Vec<u8>> {
    let mut lines = vault_text.trim_start().lines();
    let header = lines.next().unwrap_or_default();
    let fields: Vec<&str> = header.trim().split(';').collect();
    match fields.as_slice() {
        ["$ANSIBLE_VAULT", "1.1" | "1.2", "AES256", ..] => {}
        ["$ANSIBLE_VAULT", version, cipher, ..] => {
            bail!("unsupported vault format {version} with cipher {cipher}")
        }
        _ => bail!("not an ansible-vault file"),
    }

    let outer: String = lines.map(str::trim).collect();
    let inner = ctx(hex::decode(outer), || "malformed vault body".into())?;
    let parts: Vec<&[u8]> = inner.split(|&b| b == b'\n').collect();
    let [salt, tag, ciphertext] = parts.as_slice() else {
        bail!("malformed vault body: expected salt, hmac and ciphertext");
    };
    let salt = ctx(hex::decode(salt), || "malformed vault salt".into())?;
    let tag = ctx(hex::decode(tag), || "malformed vault hmac".into())?;
    let mut data = ctx(hex::decode(ciphertext), || {
        "malformed vault ciphertext".into()
    })?;

    let k = derive(password, &salt);
    if mac(&k.hmac_key, &data).verify_slice(&tag).is_err() {
        bail!("wrong vault password (HMAC mismatch)");
    }

    Aes256Ctr::new(&k.key.into(), &k.iv.into()).apply_keystream(&mut data);

    let pad = data.last().copied().unwrap_or(0) as usize;
    let valid = data.len().is_multiple_of(BLOCK)
        && (1..=BLOCK).contains(&pad)
        && data[data.len() - pad..].iter().all(|&b| b as usize == pad);
    if !valid {
        bail!("malformed vault plaintext: bad padding");
    }
    data.truncate(data.len() - pad);
    Ok(data)
}

/// Decrypt the vaulted file at `path`.
pub fn decrypt_file(path: &Path, password: &[u8]) -> Result<Vec<u8>> {
    let text = ctx(std::fs::read_to_string(path), || {
        format!("read {}", path.display())
    })?;
    ctx(decrypt(&text, password), || {
        format!("decrypt {}", path.display())
    })
}

/// Encrypt `plaintext` into `path`, mode 0600. The file is replaced in
/// one rename, so an interrupted write leaves the old one whole.
pub fn encrypt_file(path: &Path, plaintext: &[u8], password: &[u8]) -> Result<()> {
    let vaulted = encrypt(plaintext, password)?;
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    ctx(std::fs::create_dir_all(dir), || {
        format!("create {}", dir.display())
    })?;
    // A NamedTempFile is created mode 0600, and keeps it through the rename.
    let mut tmp = ctx(
        tempfile::Builder::new()
            .prefix(".ansible-vault-")
            .tempfile_in(dir),
        || format!("create a temporary file in {}", dir.display()),
    )?;
    ctx(
        tmp.write_all(vaulted.as_bytes())
            .and_then(|_| tmp.as_file().sync_all()),
        || format!("write {}", tmp.path().display()),
    )?;
    ctx(tmp.persist(path), || format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Written by `ansible-vault encrypt` (ansible-core) with password "pw".
    const FROM_ANSIBLE: &str = "$ANSIBLE_VAULT;1.1;AES256
61303561643738633836393435616565383530663530353734613037393433396634656537333636
6165653836383734376333323039613834613939383735340a313066313132303534386139623133
31346362646634313138636537333633323133663166303865326264373232393262373733303364
6431356338333437630a353932623936386232333062613166373635366163646365663461346433
63326233376166383534663734396261636436396132663735636535346434316531383135303737
6161616531393665366636633930613166633264633563383566
";

    // `ansible-vault encrypt --vault-id lbl@...`, password "pw".
    const FROM_ANSIBLE_WITH_ID: &str = "$ANSIBLE_VAULT;1.2;AES256;lbl
34663662306166323261616566396135396335316565353937376634393863616530626465653465
3234303230323662663062353936633139373936323761390a623665373137303636663864613464
32616136346564616337393037633937346231333134396433393035363231666662313061653535
3331633534373538640a653335316137316537356336633137623234636463616438353938306366
6435
";

    #[test]
    fn reads_what_ansible_vault_wrote() {
        assert_eq!(
            decrypt(FROM_ANSIBLE, b"pw").unwrap(),
            b"usecode_private_keys:\n  edge: abc=\n"
        );
        assert_eq!(decrypt(FROM_ANSIBLE_WITH_ID, b"pw").unwrap(), b"hi\n");
    }

    #[test]
    fn roundtrip() {
        for text in ["", "s3cret", "exactly16bytes!!", &"x".repeat(200)] {
            let v = encrypt(text.as_bytes(), b"pw").unwrap();
            assert!(v.starts_with("$ANSIBLE_VAULT;1.1;AES256\n"));
            assert!(v.ends_with('\n'));
            assert!(v.lines().skip(1).all(|l| l.len() <= WIDTH));
            assert!(is_encrypted(v.as_bytes()));
            assert_eq!(decrypt(&v, b"pw").unwrap(), text.as_bytes());
        }
    }

    // The layout ansible-vault reads: the body hex-decodes to three lines
    // of lower-case hex.
    #[test]
    fn writes_the_ansible_layout() {
        let v = encrypt_with_salt(b"hello", b"pw", &[7u8; 32]);
        let outer: String = v.lines().skip(1).collect();
        let inner = String::from_utf8(hex::decode(outer).unwrap()).unwrap();
        let parts: Vec<&str> = inner.split('\n').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "07".repeat(32));
        assert_eq!(parts[1].len(), 64);
        assert_eq!(parts[2].len(), 32, "one AES block");
        assert_eq!(inner, inner.to_lowercase());
    }

    #[test]
    fn wrong_password() {
        let err = decrypt(FROM_ANSIBLE, b"nope").unwrap_err().to_string();
        assert!(err.contains("wrong vault password"), "{err}");
    }

    #[test]
    fn not_a_vault() {
        assert!(!is_encrypted(b"usecode_private_keys: {}\n"));
        assert!(decrypt("junk\nAA", b"pw").is_err());
        assert!(decrypt("", b"pw").is_err());
    }

    #[test]
    fn files_round_trip_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("v.yml");
        encrypt_file(&path, b"a: 1\n", b"pw").unwrap();
        encrypt_file(&path, b"a: 2\n", b"pw").unwrap();
        assert_eq!(decrypt_file(&path, b"pw").unwrap(), b"a: 2\n");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
