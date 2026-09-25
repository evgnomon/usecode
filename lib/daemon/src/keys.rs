// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Manages this host's persistent WireGuard keypair so users never have
//! to generate or paste a private key themselves. The private key is
//! generated on first use and stored root-only on disk; only the public
//! key (safe to share) is ever meant to leave the host.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::{Command, Stdio};

use base64::Engine as _;
use x25519_dalek::{PublicKey, StaticSecret};

use crate::error::{Context, Result};

/// Where this host's WireGuard keypair is stored. It must stay
/// root-only: anyone who reads private.key can impersonate this host on
/// the tunnel.
pub const DIR: &str = "/etc/uc/wireguard";

/// The files under [`DIR`].
pub const PRIVATE_KEY_PATH: &str = "/etc/uc/wireguard/private.key";
pub const PUBLIC_KEY_PATH: &str = "/etc/uc/wireguard/public.key";

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::STANDARD;

/// This host's persistent WireGuard private key, generating and storing
/// a new keypair on first call if none exists yet.
pub fn ensure_private_key() -> Result<String> {
    if let Some(key) = read_key(PRIVATE_KEY_PATH)? {
        return Ok(key);
    }
    Ok(generate()?.0)
}

/// This host's persistent WireGuard public key, generating a keypair
/// first if one doesn't exist yet.
pub fn public_key() -> Result<String> {
    if let Some(key) = read_key(PUBLIC_KEY_PATH)? {
        return Ok(key);
    }
    Ok(generate()?.1)
}

fn read_key(path: &str) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(data) => Ok(Some(data.trim().to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(crate::err!("read {path}: {e}")),
    }
}

/// Create a new WireGuard keypair, persist it root-only under [`DIR`],
/// and return `(private_key, public_key)`. It overwrites any existing
/// keypair, so a peer's config must be updated with the new public key
/// afterwards.
pub fn generate() -> Result<(String, String)> {
    let (priv_key, pub_key) = new_pair()?;
    persist(&priv_key, &pub_key)?;
    Ok((priv_key, pub_key))
}

/// Create a WireGuard keypair without touching the filesystem. It is for
/// keys that belong to some *other* host - `uc daemon add` mints a new
/// mesh member's keypair on the control node, where [`DIR`] is neither
/// writable nor the right place for it.
pub fn new_pair() -> Result<(String, String)> {
    let priv_key = gen_private_key()?;
    let pub_key = derive_public_key(&priv_key)?;
    Ok((priv_key, pub_key))
}

fn persist(priv_key: &str, pub_key: &str) -> Result<()> {
    fs::create_dir_all(DIR).with_ctx(|| format!("create {DIR}"))?;
    // create_dir_all doesn't change the mode of a pre-existing directory.
    fs::set_permissions(DIR, fs::Permissions::from_mode(0o700))
        .with_ctx(|| format!("chmod {DIR}"))?;

    let tmp = format!("{PRIVATE_KEY_PATH}.tmp");
    write_private(&tmp, priv_key).with_ctx(|| format!("write {PRIVATE_KEY_PATH}"))?;
    fs::rename(&tmp, PRIVATE_KEY_PATH).with_ctx(|| format!("write {PRIVATE_KEY_PATH}"))?;

    // The public key is not secret, but keep it out of group/other reach
    // too so the key directory has one consistent, easily-audited mode.
    write_private(PUBLIC_KEY_PATH, pub_key).with_ctx(|| format!("write {PUBLIC_KEY_PATH}"))?;

    Ok(())
}

fn write_private(path: &str, contents: &str) -> std::io::Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(contents.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()
}

/// Uses wg(8) when available, so key generation matches exactly what the
/// rest of the WireGuard ecosystem produces, falling back to generating
/// the Curve25519 scalar directly.
fn gen_private_key() -> Result<String> {
    if let Ok(out) = Command::new("wg").arg("genkey").output() {
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).trim().to_string());
        }
    }

    let mut raw = random_bytes()?;
    raw[0] &= 248;
    raw[31] = (raw[31] & 127) | 64;
    Ok(B64.encode(raw))
}

/// Turns a base64 Curve25519 private key into its public half. It
/// prefers wg(8) so the result is byte-for-byte what the rest of the
/// WireGuard ecosystem produces, and falls back to X25519 (the same RFC
/// 7748 clamped scalar multiplication) so that a control node without
/// wireguard-tools can still mint a keypair for a host it is adding to
/// the mesh.
fn derive_public_key(priv_key: &str) -> Result<String> {
    if let Some(pub_key) = wg_pubkey(priv_key) {
        return Ok(pub_key);
    }

    let raw = B64
        .decode(priv_key.trim())
        .ctx("private key is not valid base64")?;
    let raw: [u8; 32] = raw
        .try_into()
        .map_err(|_| crate::err!("derive public key: private key is not 32 bytes"))?;
    let secret = StaticSecret::from(raw);
    Ok(B64.encode(PublicKey::from(&secret).as_bytes()))
}

fn wg_pubkey(priv_key: &str) -> Option<String> {
    let mut child = Command::new("wg")
        .arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child
        .stdin
        .take()?
        .write_all(format!("{priv_key}\n").as_bytes())
        .ok()?;
    let out = child.wait_with_output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 32 bytes straight from the kernel CSPRNG.
fn random_bytes() -> Result<[u8; 32]> {
    let mut raw = [0u8; 32];
    let mut f = fs::File::open(Path::new("/dev/urandom")).ctx("generate key")?;
    f.read_exact(&mut raw).ctx("generate key")?;
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_is_two_distinct_32_byte_keys() {
        let (priv_key, pub_key) = new_pair().unwrap();
        assert_ne!(priv_key, pub_key);
        for key in [&priv_key, &pub_key] {
            assert_eq!(B64.decode(key).unwrap().len(), 32, "{key}");
        }
    }

    // The fallback has to agree with wg(8), or a key minted on a control
    // node without wireguard-tools would not match the one its host uses.
    #[test]
    fn the_x25519_fallback_matches_a_known_vector() {
        // RFC 7748 section 6.1: Alice's private key and public key.
        let priv_key = B64.encode(
            hex("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a").as_slice(),
        );
        let want = B64.encode(
            hex("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a").as_slice(),
        );
        let raw: [u8; 32] = B64.decode(&priv_key).unwrap().try_into().unwrap();
        let got = B64.encode(PublicKey::from(&StaticSecret::from(raw)).as_bytes());
        assert_eq!(got, want);
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}
