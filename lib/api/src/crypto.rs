// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Symmetric encryption for per-user secrets stored at rest (e.g. provider
//! credentials): Fernet, keyed by the SHA-256 of `USECODE_AGENT_SECRET_KEY`.

use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE;
use serde_json::Value;
use sha2::{Digest, Sha256};

pub struct Cipher(fernet::Fernet);

impl Cipher {
    pub fn new(secret_key: &str) -> Self {
        let key = URL_SAFE.encode(Sha256::digest(secret_key.as_bytes()));
        Self(fernet::Fernet::new(&key).expect("a SHA-256 digest is a valid Fernet key"))
    }

    pub fn encrypt(&self, value: &str) -> String {
        self.0.encrypt(value.as_bytes())
    }

    pub fn decrypt(&self, token: &str) -> Result<String> {
        let plain = self.0.decrypt(token).map_err(|_| {
            anyhow::anyhow!("stored secret does not decrypt with the configured secret key")
        })?;
        String::from_utf8(plain).context("stored secret is not UTF-8")
    }

    pub fn encrypt_json(&self, value: &Value) -> String {
        self.encrypt(&value.to_string())
    }

    pub fn decrypt_json(&self, token: &str) -> Result<Value> {
        serde_json::from_str(&self.decrypt(token)?).context("stored secret is not JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let cipher = Cipher::new("secret");
        let value = serde_json::json!({"apiKey": "abc"});
        assert_eq!(
            cipher.decrypt_json(&cipher.encrypt_json(&value)).unwrap(),
            value
        );
    }

    #[test]
    fn decrypts_tokens_minted_by_python_cryptography() {
        // Fernet(urlsafe_b64encode(sha256(b"secret").digest())).encrypt(b'{"apiKey": "abc"}')
        let token = "gAAAAABqtqf5jtVKjL6jIvOfFkc137aexfQQGNU338ZjG7RZv321Hu9LzbW3gWeeuXYnQzsdDAfWcXSdZcsBVfujPaKj9cSNa3ADDIgIQmg456m68z2FI3k=";
        let cipher = Cipher::new("secret");
        assert_eq!(cipher.decrypt(token).unwrap(), r#"{"apiKey": "abc"}"#);
    }
}
