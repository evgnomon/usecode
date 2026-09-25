// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! SSH ed25519 key helpers.

use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ssh_key::private::KeypairData;
use ssh_key::public::KeyData;
use ssh_key::{PrivateKey, PublicKey};

/// Raw 32-byte ed25519 public key.
pub type RawPub = [u8; 32];

/// Expand a leading `~` / `~/` to `$HOME`.
pub fn expand_user(path: &str) -> PathBuf {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if path == "~" {
        if let Some(h) = home() {
            return h;
        }
    } else if let Some(rest) = path.strip_prefix("~/")
        && let Some(h) = home()
    {
        return h.join(rest);
    }
    PathBuf::from(path)
}

/// Parse an OpenSSH public key line (`ssh-ed25519 AAAA… [comment]`).
/// `Ok(None)` means a valid key of another type.
pub fn parse_openssh_pub(line: &str) -> Result<Option<RawPub>> {
    let key = PublicKey::from_openssh(line.trim()).map_err(|e| anyhow!("{e}"))?;
    Ok(match key.key_data() {
        KeyData::Ed25519(k) => Some(k.0),
        _ => None,
    })
}

/// Load an ed25519 public key from an OpenSSH `.pub` file.
pub fn load_pubkey(path: &str) -> Result<RawPub> {
    let path = expand_user(path);
    if !path.is_file() {
        bail!("Public key not found: {}", path.display());
    }
    let data = std::fs::read_to_string(&path)?;
    parse_openssh_pub(&data)?.ok_or_else(|| anyhow!("Only ed25519 keys are supported"))
}

/// Load the 32-byte seed of an unencrypted OpenSSH ed25519 private key.
pub fn load_priv_seed(path: &str) -> Result<[u8; 32]> {
    let path = expand_user(path);
    if !path.is_file() {
        bail!("Private key not found: {}", path.display());
    }
    let data = std::fs::read_to_string(&path)?;
    let key = PrivateKey::from_openssh(data.trim()).map_err(|e| anyhow!("{e}"))?;
    if key.is_encrypted() {
        bail!("Key is password-protected.");
    }
    match key.key_data() {
        KeypairData::Ed25519(kp) => Ok(kp.private.to_bytes()),
        _ => bail!("Only ed25519 keys are supported"),
    }
}

/// OpenSSH text form of a public key, without comment.
pub fn openssh_string(raw: &RawPub) -> String {
    let mut blob = Vec::with_capacity(51);
    for part in [b"ssh-ed25519".as_slice(), raw.as_slice()] {
        blob.extend_from_slice(&(part.len() as u32).to_be_bytes());
        blob.extend_from_slice(part);
    }
    format!("ssh-ed25519 {}", B64.encode(blob))
}

/// Sign `message`, returning the base64 signature.
pub fn sign(seed: &[u8; 32], message: &[u8]) -> String {
    B64.encode(SigningKey::from_bytes(seed).sign(message).to_bytes())
}

/// Verify a base64 signature; any decoding problem counts as invalid.
pub fn verify(public: &RawPub, sig_b64: &str, message: &[u8]) -> bool {
    let Ok(sig) = B64.decode(sig_b64.trim()) else {
        return false;
    };
    let Ok(sig) = Signature::from_slice(&sig) else {
        return false;
    };
    let Ok(vk) = VerifyingKey::from_bytes(public) else {
        return false;
    };
    vk.verify(message, &sig).is_ok()
}

/// Public key of a seed.
#[cfg(test)]
pub fn public_of(seed: &[u8; 32]) -> RawPub {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openssh_roundtrip() {
        let raw = public_of(&[9u8; 32]);
        let s = openssh_string(&raw);
        assert!(s.starts_with("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI"));
        assert_eq!(parse_openssh_pub(&s).unwrap(), Some(raw));
        assert_eq!(
            parse_openssh_pub(&format!("{s} user@host\n")).unwrap(),
            Some(raw)
        );
    }

    #[test]
    fn sign_verify() {
        let seed = [3u8; 32];
        let pk = public_of(&seed);
        let sig = sign(&seed, b"msg");
        assert!(verify(&pk, &sig, b"msg"));
        assert!(!verify(&pk, &sig, b"other"));
        assert!(!verify(&pk, "not base64!", b"msg"));
    }

    #[test]
    fn expands_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            expand_user("~/.ssh/x"),
            PathBuf::from(format!("{home}/.ssh/x"))
        );
        assert_eq!(expand_user("/abs"), PathBuf::from("/abs"));
    }
}
