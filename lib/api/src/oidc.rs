// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! OpenID Connect: usecode.dev as the login of other sites, Forgejo first.
//!
//! The routes are in `routes/oidc.rs`; this is what they stand on: the
//! clients, the key tokens are signed with, the tokens, and the codes.
//!
//! - A client is a file in [`Settings::oidc_clients_dir`] named by its id
//!   (on the cluster, a key of the console/oidc-clients secret): its secret
//!   and the addresses it may be sent back to. Read on each use, so a new
//!   one works without a restart.
//! - Tokens are ES256 JWTs. The key is made from the API's secret key, so
//!   every instance signs with the same one and nothing new is kept.
//! - The access token is a JWT too, for the userinfo endpoint only: it
//!   holds no more than the ID token, and runs out as soon.

use std::path::Path;

use anyhow::{Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use p256::ecdsa::signature::{Signer, Verifier};
use p256::ecdsa::{Signature, SigningKey, VerifyingKey};
use rand::Rng;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::config::Settings;
use crate::schema;
use crate::store::{Store, Who, hash, token_urlsafe};

/// How long a code works: only for the site's server to swap it, at once.
pub const CODE_SECONDS: i64 = 120;
/// How long the tokens work.
pub const TOKEN_SECONDS: i64 = 3600;

/// A site that logs in with usecode.dev.
#[derive(Debug, Clone, Deserialize)]
pub struct Client {
    pub secret: String,
    pub redirect_uris: Vec<String>,
}

impl Client {
    /// Whether `secret` is this client's, in constant time.
    pub fn is_secret(&self, secret: &str) -> bool {
        !self.secret.is_empty() && bool::from(self.secret.as_bytes().ct_eq(secret.as_bytes()))
    }
}

/// The client `id`, if there is one.
pub fn client(settings: &Settings, id: &str) -> Result<Option<Client>> {
    // A key of a Kubernetes secret: letters, digits, `-`, `_` and `.`; and
    // never the volume's own `..data` and the like.
    let fine = !id.is_empty()
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if !fine {
        return Ok(None);
    }
    let path = Path::new(&settings.oidc_clients_dir).join(id);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => bail!("reading {}: {err}", path.display()),
    };
    match serde_json::from_str(&text) {
        Ok(client) => Ok(Some(client)),
        Err(err) => bail!("{} isn't a client: {err}", path.display()),
    }
}

/// The key tokens are signed with.
pub struct Keys {
    signing: SigningKey,
    verifying: VerifyingKey,
    kid: String,
}

impl Keys {
    /// The key made from the API's secret key: the same on every instance.
    pub fn new(secret_key: &str) -> Keys {
        for counter in 0u32.. {
            let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes())
                .expect("HMAC takes a key of any length");
            mac.update(b"usecode.dev OpenID Connect signing key");
            mac.update(&counter.to_be_bytes());
            // Only a value of 0 or past the curve's order isn't a key: next
            // to never, so the next counter is there for it.
            if let Ok(signing) = SigningKey::from_slice(&mac.finalize().into_bytes()) {
                let verifying = *signing.verifying_key();
                let point = verifying.to_encoded_point(false);
                let kid = URL_SAFE_NO_PAD.encode(&Sha256::digest(point.as_bytes())[..12]);
                return Keys {
                    signing,
                    verifying,
                    kid,
                };
            }
        }
        unreachable!("a 32 bit counter runs out first")
    }

    /// The public key, as a JWK.
    pub fn jwk(&self) -> Value {
        let point = self.verifying.to_encoded_point(false);
        let coordinate =
            |c: Option<&p256::FieldBytes>| URL_SAFE_NO_PAD.encode(c.expect("uncompressed"));
        json!({
            "kty": "EC",
            "crv": "P-256",
            "x": coordinate(point.x()),
            "y": coordinate(point.y()),
            "use": "sig",
            "alg": "ES256",
            "kid": self.kid,
        })
    }

    /// `claims`, as a signed JWT.
    pub fn sign(&self, claims: &Value) -> String {
        let header = json!({"alg": "ES256", "typ": "JWT", "kid": self.kid});
        let input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(header.to_string()),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        );
        let signature: Signature = self.signing.sign(input.as_bytes());
        format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
    }

    /// The claims of a JWT this key signed, or `None`. Whether they still
    /// hold (`exp`, `aud`) is the caller's to check.
    pub fn verify(&self, token: &str) -> Option<Map<String, Value>> {
        let (input, signature) = token.rsplit_once('.')?;
        let (header, payload) = input.split_once('.')?;
        let decode = |part: &str| -> Option<Value> {
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).ok()?).ok()
        };
        if decode(header)?["alg"] != "ES256" {
            return None;
        }
        let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(signature).ok()?).ok()?;
        self.verifying.verify(input.as_bytes(), &signature).ok()?;
        match decode(payload)? {
            Value::Object(claims) => Some(claims),
            _ => None,
        }
    }
}

/// What the person is, as the tokens and userinfo say it. Every client is
/// one we set up, so each gets the email and the username whatever scope it
/// asked for: Forgejo can't make an account without them.
pub fn claims(user_id: &Uuid, who: &Who) -> Map<String, Value> {
    let mut claims = Map::new();
    claims.insert("sub".into(), json!(user_id.to_string()));
    if let Some(email) = &who.email {
        claims.insert("email".into(), json!(email));
        // An email account is only made once a code mailed to it is typed in.
        claims.insert("email_verified".into(), json!(true));
    }
    if let Some(username) = &who.username {
        for claim in ["preferred_username", "nickname", "name"] {
            claims.insert(claim.into(), json!(username));
        }
    }
    claims
}

/// What PKCE's S256 makes of a verifier.
pub fn s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// A login on its way back to a client: what the code stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub client_id: String,
    pub user_id: Uuid,
    pub redirect_uri: String,
    pub scope: String,
    pub nonce: Option<String>,
    pub code_challenge: Option<String>,
}

impl Store {
    /// A code for `grant`, good for [`CODE_SECONDS`] and one swap.
    pub async fn start_oauth_code(&self, grant: &Grant) -> Result<String> {
        let code = token_urlsafe();
        let code_hash = hash(&code);
        let pool = self.pool_for_key(schema::OAUTH_CODES, &code_hash).await?;
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO oauth_codes
                 (code_hash, client_id, user_id, redirect_uri, scope, nonce, code_challenge,
                  expires_at, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(&code_hash)
        .bind(&grant.client_id)
        .bind(grant.user_id)
        .bind(&grant.redirect_uri)
        .bind(&grant.scope)
        .bind(&grant.nonce)
        .bind(&grant.code_challenge)
        .bind(now + chrono::Duration::seconds(CODE_SECONDS))
        .bind(now)
        .execute(&pool)
        .await?;
        if rand::rng().random_ratio(1, 20) {
            sqlx::query("DELETE FROM oauth_codes WHERE expires_at < $1")
                .bind(now)
                .execute(&pool)
                .await?;
        }
        Ok(code)
    }

    /// The grant behind `code`, used up: a second swap finds nothing.
    pub async fn take_oauth_code(&self, code: &str) -> Result<Option<Grant>> {
        if code.is_empty() {
            return Ok(None);
        }
        let code_hash = hash(code);
        let pool = self.pool_for_key(schema::OAUTH_CODES, &code_hash).await?;
        type Row = (
            String,
            Uuid,
            String,
            String,
            Option<String>,
            Option<String>,
            DateTime<Utc>,
        );
        let row: Option<Row> = sqlx::query_as(
            "DELETE FROM oauth_codes WHERE code_hash = $1
             RETURNING client_id, user_id, redirect_uri, scope, nonce, code_challenge, expires_at",
        )
        .bind(&code_hash)
        .fetch_optional(&pool)
        .await?;
        Ok(row.and_then(
            |(client_id, user_id, redirect_uri, scope, nonce, code_challenge, expires_at)| {
                (expires_at > Utc::now()).then_some(Grant {
                    client_id,
                    user_id,
                    redirect_uri,
                    scope,
                    nonce,
                    code_challenge,
                })
            },
        ))
    }

    /// Who `user_id` is, or `None` when there's no such user.
    pub async fn who(&self, user_id: &Uuid) -> Result<Option<Who>> {
        let users = self.pool_for_user(user_id).await?;
        Ok(
            sqlx::query_as("SELECT phone, email, username FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&users)
                .await?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_check_with_the_key_that_signed_them() {
        let keys = Keys::new("a secret");
        let token = keys.sign(&json!({"sub": "ada", "n": 1}));
        let claims = keys.verify(&token).expect("it checks");
        assert_eq!(claims["sub"], "ada");
        // The same secret makes the same key, on every instance.
        assert!(Keys::new("a secret").verify(&token).is_some());
        assert_eq!(Keys::new("a secret").jwk(), keys.jwk());
        assert!(Keys::new("another secret").verify(&token).is_none());
        // Changed claims don't check.
        let (header, rest) = token.split_once('.').unwrap();
        let (_, signature) = rest.split_once('.').unwrap();
        let forged = URL_SAFE_NO_PAD.encode(json!({"sub": "bob", "n": 1}).to_string());
        assert!(
            keys.verify(&format!("{header}.{forged}.{signature}"))
                .is_none()
        );
        assert!(keys.verify("not.a.token").is_none());
        let jwk = keys.jwk();
        assert_eq!(jwk["crv"], "P-256");
        assert_eq!(
            URL_SAFE_NO_PAD
                .decode(jwk["x"].as_str().unwrap())
                .unwrap()
                .len(),
            32
        );
    }

    #[test]
    fn pkce_s256() {
        // RFC 7636, appendix B.
        assert_eq!(
            s256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_client_id_is_a_file_name_and_nothing_more() {
        let dir = std::env::temp_dir().join(format!("oidc-clients-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("forgejo"),
            r#"{"secret": "s3cret", "redirect_uris": ["https://git.example.com/cb"]}"#,
        )
        .unwrap();
        let mut settings = Settings::from_env().unwrap();
        settings.oidc_clients_dir = dir.display().to_string();
        let forgejo = client(&settings, "forgejo").unwrap().unwrap();
        assert!(forgejo.is_secret("s3cret"));
        assert!(!forgejo.is_secret("s3cre"));
        assert!(!forgejo.is_secret(""));
        for id in ["", "nobody", "..data", "../forgejo", "a/b"] {
            assert!(client(&settings, id).unwrap().is_none(), "{id}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
