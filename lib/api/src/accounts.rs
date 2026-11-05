// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Email accounts: the password, the code mailed to finish a sign up, a
//! log in or a reset, and the storage behind them.
//!
//! A sign up or a log in takes the email and password first, then a six
//! digit code mailed to the address, good for [`CODE_MINUTES`] and
//! [`MAX_TRIES`] guesses. The account is only made once the code is right.
//! A reset takes the address and a new password, then the code; an address
//! without an account gets the same answers and no mail.
//!
//! Passwords are scrypt hashes in the `scrypt$N$r$p$salt$hash` form the
//! console used, so its accounts carry over as they are.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use rand::{Rng, RngCore};
use subtle::ConstantTimeEq;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::db::Partition;
use crate::schema;
use crate::store::{Store, hash, token_urlsafe};

/// How long a browser stays logged in.
pub const SESSION_DAYS: i64 = 30;
/// How long a mailed code works.
pub const CODE_MINUTES: i64 = 10;
/// Wrong codes a challenge takes before it's gone.
pub const MAX_TRIES: i32 = 5;

// scrypt's cost: N = 2^15, r = 8, p = 1, as the console had it.
const SCRYPT_LOG_N: u8 = 15;
const SCRYPT_R: u32 = 8;
const SCRYPT_P: u32 = 1;
const KEY_LEN: usize = 32;

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// Why `email` and `password` can't make an account (or be its new
/// password), or `None` when they can.
pub fn signup_problem(email: &str, password: &str) -> Option<&'static str> {
    let looks_right = || {
        if email.len() > 254 || email.chars().any(char::is_whitespace) {
            return false;
        }
        let Some((local, domain)) = email.split_once('@') else {
            return false;
        };
        !local.is_empty()
            && !domain.contains('@')
            && domain
                .char_indices()
                .any(|(i, c)| c == '.' && i > 0 && i + 1 < domain.len())
    };
    if !looks_right() {
        return Some("That doesn't look like an email address.");
    }
    let length = password.chars().count();
    if length < 10 {
        return Some("Pick a password of at least 10 characters.");
    }
    if length > 1024 {
        return Some("That password is a bit too long.");
    }
    None
}

fn derive(password: &str, salt: &[u8], log_n: u8, r: u32, p: u32) -> Result<Vec<u8>> {
    let params = scrypt::Params::new(log_n, r, p, KEY_LEN)
        .map_err(|err| anyhow::anyhow!("scrypt parameters: {err}"))?;
    let password: String = password.nfkc().collect();
    let mut key = vec![0u8; KEY_LEN];
    scrypt::scrypt(password.as_bytes(), salt, &params, &mut key)
        .map_err(|err| anyhow::anyhow!("scrypt: {err}"))?;
    Ok(key)
}

/// `scrypt$N$r$p$salt$hash`, base64url, so the cost can rise later and
/// old hashes still check. Slow on purpose: call it off the async threads.
pub fn hash_password(password: &str) -> Result<String> {
    let mut salt = [0u8; 16];
    rand::rng().fill_bytes(&mut salt);
    let key = derive(password, &salt, SCRYPT_LOG_N, SCRYPT_R, SCRYPT_P)?;
    Ok(format!(
        "scrypt${}${SCRYPT_R}${SCRYPT_P}${}${}",
        1u64 << SCRYPT_LOG_N,
        URL_SAFE_NO_PAD.encode(salt),
        URL_SAFE_NO_PAD.encode(key)
    ))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    let ["scrypt", n, r, p, salt, want] = parts.as_slice() else {
        return false;
    };
    let (Ok(n), Ok(r), Ok(p)) = (n.parse::<u64>(), r.parse::<u32>(), p.parse::<u32>()) else {
        return false;
    };
    if !n.is_power_of_two() || n < 2 {
        return false;
    }
    let (Ok(salt), Ok(want)) = (URL_SAFE_NO_PAD.decode(salt), URL_SAFE_NO_PAD.decode(want)) else {
        return false;
    };
    match derive(password, &salt, n.trailing_zeros() as u8, r, p) {
        Ok(got) => got.len() == want.len() && bool::from(got.ct_eq(&want)),
        Err(_) => false,
    }
}

/// [`hash_password`] on a blocking thread.
pub async fn hash_password_async(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&password)).await?
}

/// [`verify_password`] on a blocking thread. Without a stored hash it
/// checks against a made-up one, so a login takes as long either way.
pub async fn verify_password_async(password: String, stored: Option<String>) -> Result<bool> {
    tokio::task::spawn_blocking(move || match stored {
        Some(stored) => verify_password(&password, &stored),
        None => {
            static DUMMY: OnceLock<String> = OnceLock::new();
            let dummy = DUMMY.get_or_init(|| hash_password(&token_urlsafe()).unwrap_or_default());
            verify_password(&password, dummy);
            false
        }
    })
    .await
    .map_err(Into::into)
}

/// Six digits from the OS random source, leading zeros kept.
pub fn new_code() -> String {
    format!("{:06}", rand::rng().random_range(0..1_000_000))
}

/// Strips the spaces and dashes people type or paste a code with.
pub fn clean_code(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect()
}

fn code_hash(token: &str, code: &str) -> String {
    hash(&format!("{token}\0{code}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Signup,
    Login,
    Reset,
}

impl Purpose {
    fn as_str(self) -> &'static str {
        match self {
            Purpose::Signup => "signup",
            Purpose::Login => "login",
            Purpose::Reset => "reset",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "signup" => Some(Purpose::Signup),
            "login" => Some(Purpose::Login),
            "reset" => Some(Purpose::Reset),
            _ => None,
        }
    }
}

/// What a code finishes once it is right.
#[derive(Debug, Clone)]
pub struct Pending {
    pub purpose: Purpose,
    pub email: String,
    /// Who logs in or resets; a sign up has no user yet, nor a reset for an
    /// address without an account.
    pub user_id: Option<Uuid>,
    /// The new password's hash, for a sign up or a reset.
    pub password_hash: Option<String>,
}

#[derive(Debug)]
pub enum Checked {
    Ok(Pending),
    /// Run out, used up, or never there.
    Gone,
    Wrong,
}

/// The mail a code goes out in: `(subject, text)`.
pub fn message(purpose: Purpose, code: &str) -> (String, String) {
    let (what, ignore) = match purpose {
        Purpose::Signup => (
            "finish signing up to",
            "If you didn't sign up, someone typed your address by mistake. Ignore this mail and no account is made.",
        ),
        Purpose::Login => (
            "log in to",
            "If this wasn't you, someone knows your password. Don't share this code with anyone.",
        ),
        Purpose::Reset => (
            "set a new password on",
            "If you didn't ask for this, ignore this mail: your password stays as it is. Don't share this code with anyone.",
        ),
    };
    (
        format!("{code} is your usecode.dev code"),
        format!(
            "Hi,\n\nYour code to {what} usecode.dev is:\n\n    {code}\n\nIt works for {CODE_MINUTES} minutes.\n\n{ignore}\n\nusecode.dev\n"
        ),
    )
}

/// Counts per key in a sliding window, so a password can't be guessed at
/// speed, nor an inbox flooded with codes. Per instance.
pub struct Limiter {
    max: usize,
    window: Duration,
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl Limiter {
    pub fn new(max: usize, window: Duration) -> Self {
        Self {
            max,
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    pub fn blocked(&self, key: &str) -> bool {
        let mut hits = self.hits.lock().unwrap();
        let now = Instant::now();
        let recent: Vec<Instant> = hits
            .get(key)
            .map(|times| {
                times
                    .iter()
                    .copied()
                    .filter(|t| now.duration_since(*t) < self.window)
                    .collect()
            })
            .unwrap_or_default();
        let blocked = recent.len() >= self.max;
        if recent.is_empty() {
            hits.remove(key);
        } else {
            hits.insert(key.to_string(), recent);
        }
        blocked
    }

    pub fn fail(&self, key: &str) {
        self.hits
            .lock()
            .unwrap()
            .entry(key.to_string())
            .or_default()
            .push(Instant::now());
    }

    pub fn clear(&self, key: &str) {
        self.hits.lock().unwrap().remove(key);
    }
}

/// The limits the account endpoints keep.
pub struct Limits {
    /// Failed logins per email.
    pub login: Limiter,
    /// Codes mailed per address.
    pub mailed: Limiter,
}

impl Default for Limits {
    fn default() -> Self {
        let window = Duration::from_secs(15 * 60);
        Self {
            login: Limiter::new(10, window),
            mailed: Limiter::new(5, window),
        }
    }
}

/// An email account, as logging in needs it.
#[derive(Debug, Clone)]
pub struct EmailUser {
    pub id: Uuid,
    pub email: String,
    pub password_hash: Option<String>,
}

impl Store {
    /// The user with `email`, through the email directory.
    pub async fn find_email_user(&self, email: &str) -> Result<Option<EmailUser>> {
        let directory = self.pool_for_key(schema::EMAIL_DIRECTORY, email).await?;
        let user_id: Option<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM email_directory WHERE email = $1")
                .bind(email)
                .fetch_optional(&directory)
                .await?;
        let Some(id) = user_id else {
            return Ok(None);
        };
        let users = self.pool_for_user(&id).await?;
        let row: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT email, password_hash FROM users WHERE id = $1")
                .bind(id)
                .fetch_optional(&users)
                .await?;
        Ok(row.map(|(stored, password_hash)| EmailUser {
            id,
            email: stored.unwrap_or_else(|| email.to_string()),
            password_hash,
        }))
    }

    /// A new account for `email`, or `None` when the address has one.
    pub async fn create_email_user(
        &self,
        email: &str,
        password_hash: &str,
        created_at: DateTime<Utc>,
    ) -> Result<Option<Uuid>> {
        // The user row first, as for phones (see `get_or_create_user`): a
        // crash in between leaves an inert row, not a directory entry
        // pointing at nobody.
        let id = Uuid::new_v4();
        let users = self.pool_for_user(&id).await?;
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, created_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(email)
        .bind(password_hash)
        .bind(created_at)
        .execute(&users)
        .await?;
        let directory = self.pool_for_key(schema::EMAIL_DIRECTORY, email).await?;
        let registered: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO email_directory (email, user_id, created_at) VALUES ($1, $2, $3)
             ON CONFLICT (email) DO NOTHING RETURNING user_id",
        )
        .bind(email)
        .bind(id)
        .bind(Utc::now())
        .fetch_optional(&directory)
        .await?;
        if registered.is_some() {
            return Ok(Some(id));
        }
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(&users)
            .await?;
        Ok(None)
    }

    /// A new password, and every session of the user logged out.
    pub async fn set_password(&self, user_id: &Uuid, password_hash: &str) -> Result<()> {
        let users = self.pool_for_user(user_id).await?;
        sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
            .bind(user_id)
            .bind(password_hash)
            .execute(&users)
            .await?;
        self.end_sessions(user_id).await
    }

    /// Log the user out everywhere: revoke the keys that run out, leaving
    /// their API keys alone.
    pub async fn end_sessions(&self, user_id: &Uuid) -> Result<()> {
        let users = self.pool_for_user(user_id).await?;
        let hashes: Vec<String> =
            sqlx::query_scalar("SELECT key_hash FROM user_api_keys WHERE user_id = $1")
                .bind(user_id)
                .fetch_all(&users)
                .await?;
        let mut by_partition: Vec<(Partition, Vec<String>)> = Vec::new();
        for key_hash in hashes {
            let partition = self
                .db()
                .partition_for_key(schema::API_KEYS, &key_hash)
                .await?;
            match by_partition.iter_mut().find(|(p, _)| *p == partition) {
                Some((_, group)) => group.push(key_hash),
                None => by_partition.push((partition, vec![key_hash])),
            }
        }
        for (partition, group) in by_partition {
            let keys = self.db().pool(&partition).await?;
            sqlx::query(
                "UPDATE api_keys SET revoked_at = $3
                 WHERE key_hash = ANY($1) AND user_id = $2
                   AND expires_at IS NOT NULL AND revoked_at IS NULL",
            )
            .bind(&group)
            .bind(user_id)
            .bind(Utc::now())
            .execute(&keys)
            .await?;
        }
        Ok(())
    }

    /// A new challenge: the token goes back to the browser, the code into
    /// the mail.
    pub async fn start_code(&self, p: &Pending) -> Result<(String, String)> {
        let token = token_urlsafe();
        let code = new_code();
        let token_hash = hash(&token);
        let pool = self.pool_for_key(schema::EMAIL_CODES, &token_hash).await?;
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO email_codes
                 (token_hash, purpose, email, user_id, password_hash, code_hash, expires_at, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&token_hash)
        .bind(p.purpose.as_str())
        .bind(&p.email)
        .bind(p.user_id)
        .bind(&p.password_hash)
        .bind(code_hash(&token, &code))
        .bind(now + chrono::Duration::minutes(CODE_MINUTES))
        .bind(now)
        .execute(&pool)
        .await?;
        // Clearing out old codes now and then keeps the table small.
        if rand::rng().random_ratio(1, 20) {
            sqlx::query("DELETE FROM email_codes WHERE expires_at < $1")
                .bind(now)
                .execute(&pool)
                .await?;
        }
        Ok((token, code))
    }

    /// Check `code` against the challenge `token` names. A right code uses
    /// the challenge up; each wrong one counts, and the last allowed one
    /// ends it.
    pub async fn check_code(&self, token: &str, code: &str) -> Result<Checked> {
        if token.is_empty() {
            return Ok(Checked::Gone);
        }
        let token_hash = hash(token);
        let pool = self.pool_for_key(schema::EMAIL_CODES, &token_hash).await?;
        type Row = (String, String, Option<Uuid>, Option<String>, String, i32);
        let row: Option<Row> = sqlx::query_as(
            "UPDATE email_codes SET attempts = attempts + 1
             WHERE token_hash = $1 AND expires_at > $2 AND attempts < $3
             RETURNING purpose, email, user_id, password_hash, code_hash, attempts",
        )
        .bind(&token_hash)
        .bind(Utc::now())
        .bind(MAX_TRIES)
        .fetch_optional(&pool)
        .await?;
        let Some((purpose, email, user_id, password_hash, want, attempts)) = row else {
            return Ok(Checked::Gone);
        };
        let Some(purpose) = Purpose::parse(&purpose) else {
            bail!("email_codes has a code for {purpose:?}");
        };
        let wrong_or_gone = || {
            if attempts >= MAX_TRIES {
                Checked::Gone
            } else {
                Checked::Wrong
            }
        };
        let got = code_hash(token, &clean_code(code));
        if !bool::from(got.as_bytes().ct_eq(want.as_bytes())) {
            if attempts >= MAX_TRIES {
                sqlx::query("DELETE FROM email_codes WHERE token_hash = $1")
                    .bind(&token_hash)
                    .execute(&pool)
                    .await?;
            }
            return Ok(wrong_or_gone());
        }
        // A reset for nobody: its code was never mailed. Should it be
        // guessed anyway, it still only runs out.
        if purpose == Purpose::Reset && user_id.is_none() {
            return Ok(wrong_or_gone());
        }
        // Only one request gets to use it, however many arrive together.
        let used = sqlx::query("DELETE FROM email_codes WHERE token_hash = $1")
            .bind(&token_hash)
            .execute(&pool)
            .await?;
        if used.rows_affected() == 0 {
            return Ok(Checked::Gone);
        }
        Ok(Checked::Ok(Pending {
            purpose,
            email,
            user_id,
            password_hash,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_console_hash_still_checks() {
        // Made by the console's server/auth.ts for "correct horse battery".
        let console =
            "scrypt$32768$8$1$zxREC_OdG3ab9b9wy1aDoQ$iofY9IrjxUq4zXlK4EbMZHsR__LMqySBLVsyfxx6gpU";
        assert!(verify_password("correct horse battery", console));
        assert!(!verify_password("correct horse batter", console));
        let stored = hash_password("correct horse battery").unwrap();
        assert!(stored.starts_with("scrypt$32768$8$1$"));
        assert!(verify_password("correct horse battery", &stored));
        assert!(!verify_password("x", "bcrypt$whatever"));
    }

    #[test]
    fn passwords_are_normalized() {
        let stored = hash_password("cafe\u{301} au lait!").unwrap();
        assert!(verify_password("caf\u{e9} au lait!", &stored));
    }

    #[test]
    fn signup_checks() {
        assert_eq!(signup_problem("a@b.co", "0123456789"), None);
        assert!(signup_problem("a@b", "0123456789").is_some());
        assert!(signup_problem("a b@c.de", "0123456789").is_some());
        assert!(signup_problem("a@.de", "0123456789").is_some());
        assert!(signup_problem("a@b.co", "short").is_some());
        assert_eq!(normalize_email("  Ada@Example.COM "), "ada@example.com");
    }

    #[test]
    fn codes_are_six_digits_and_forgiving() {
        for _ in 0..100 {
            let code = new_code();
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|b| b.is_ascii_digit()));
        }
        assert_eq!(clean_code(" 123-456 "), "123456");
    }

    #[test]
    fn the_limiter_counts_in_its_window() {
        let limiter = Limiter::new(2, Duration::from_secs(60));
        assert!(!limiter.blocked("a"));
        limiter.fail("a");
        limiter.fail("a");
        assert!(limiter.blocked("a"));
        assert!(!limiter.blocked("b"));
        limiter.clear("a");
        assert!(!limiter.blocked("a"));
    }
}
