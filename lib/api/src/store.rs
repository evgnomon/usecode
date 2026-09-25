// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Row storage, split across the database instances described in `db`.
//!
//! **Every table here is partitioned; none is global.** A table with no
//! foreign key hashes its own key and reads its own map, so every lookup in
//! this module names the table it is resolving against:
//!
//! - `Db::partition_for_key(table, key)` — the phone number against
//!   `user_directory`'s or `otps`' map, the user id against `users`'.
//! - `Db::partition_for_user(user_id)` — the `users` shorthand, used for the
//!   tables whose first foreign key leads back to a user (web sessions,
//!   provider credentials, servers, and the `user_api_keys` / `user_tasks`
//!   indexes). Those inherit their owner's instance and have no map of their
//!   own, which is exactly why they can carry real foreign keys to `users`.
//! - `Db::partition_for_table(table)` — the catalog tables, whose map is one
//!   whole-space range because they are read by scans over non-key
//!   predicates rather than by their key.
//!
//! A method touching a table with a foreign key either resolves the
//! partition from a user id or takes the partition as its first argument:
//! such a row is unreachable without it, which is the point. Callers get it
//! from the authenticated user (`ApiKeyRecord::partition` /
//! `WebSessionRecord::partition`).
//!
//! Two things are looked up by their own hash rather than by a user id, and
//! they are addressed differently because they arrive at different moments:
//!
//! - An **API key** arrives before any user id exists to hash, so `api_keys`
//!   is a root table partitioned on the key hash — the token alone routes
//!   authentication to one instance. `user_api_keys`, which does live with
//!   the user, is what turns a user id back into their key hashes for
//!   listing and revoking.
//! - A **web session token** is only ever handed out after the user is
//!   known, so `web_sessions` stays a child of `users` and the token is
//!   minted with the owner's virtual shard as a short hex prefix
//!   (`"1a2b.<secret>"`), which routes the cookie lookup to the owner's
//!   instance.
//!
//! A **task** is a third case, hashed on neither a token nor a user id but
//! on its `assignee` — the API instance carrying it — because that is the
//! query that has to be cheap: each instance sweeps its own outstanding
//! work, and hashing the node name puts all of it on one database. Task
//! methods here take the assignee and resolve the partition themselves;
//! `user_tasks` is the index that turns a user id back into (task id,
//! assignee).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::types::Json;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::db::{self, Db, Partition};
use crate::schema;

pub fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

/// Seconds since the epoch, as the API has always exposed timestamps.
pub fn ts(value: DateTime<Utc>) -> f64 {
    value.timestamp_micros() as f64 / 1e6
}

/// A URL-safe random token carrying 32 bytes of entropy.
pub fn token_urlsafe() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

// A web session token is looked up by its own hash, but its row is a child
// of `users`, so on its own the token says nothing about where that row is.
// It is therefore minted with the owner's virtual shard in front —
// "1a2b.<secret>" — which routes the lookup to exactly one instance. The
// hash stored in the database covers the whole token, prefix included.

const TOKEN_PREFIX_WIDTH: usize = 4; // hex digits, enough for db::VIRTUAL_SHARDS - 1

fn mint_session_token(user_id: &Uuid) -> String {
    let bucket = db::virtual_shard_uuid(user_id);
    format!(
        "{bucket:0width$x}.{}",
        token_urlsafe(),
        width = TOKEN_PREFIX_WIDTH
    )
}

/// The owner's `users` bucket carried in a session token's prefix. Tokens
/// minted before the prefix existed have no separator at all (the random
/// part never contains a "."); a deployment old enough to hold them never
/// had more than one instance, so the main database — `None` — is the only
/// place such a row can be.
fn session_token_bucket(token: &str) -> Option<u32> {
    let (prefix, secret) = token.split_once('.')?;
    if secret.is_empty() || prefix.len() != TOKEN_PREFIX_WIDTH {
        return None;
    }
    u32::from_str_radix(prefix, 16).ok()
}

#[derive(Debug, Clone)]
pub struct OtpRecord {
    pub expires_at: f64,
    pub resend_after: f64,
    pub attempts: i32,
}

#[derive(Debug, Clone)]
pub struct ApiKeyRecord {
    // The key's hash, which is both its identity and its address: it is what
    // routes to the instance holding it, and what the API exposes as the
    // key's id. The token itself is returned exactly once, at issue time.
    pub id: String,
    pub user_id: Uuid,
    pub phone: String,
    // The database instance holding this *user's* partitioned rows; passed
    // to every partitioned store call made on their behalf. Not where the
    // api_keys row itself is — that one hashes on the key hash.
    pub partition: Partition,
    pub label: String,
    pub created_at: f64,
    pub last_used_at: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct WebSessionRecord {
    pub user_id: Uuid,
    pub phone: String,
    pub partition: Partition,
    pub api_key_hash: String,
    pub created_at: f64,
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    key_hash: String,
    user_id: Uuid,
    label: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
}

impl ApiKeyRow {
    fn record(self, phone: String, partition: Partition) -> ApiKeyRecord {
        ApiKeyRecord {
            id: self.key_hash,
            user_id: self.user_id,
            phone,
            partition,
            label: self.label,
            created_at: ts(self.created_at),
            last_used_at: self.last_used_at.map(ts),
        }
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ServerRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub provider_server_id: String,
    pub r#type: String,
    pub name: String,
    pub status: String,
    pub public_ip4: Option<String>,
    pub public_ip6: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TaskRow {
    id: Uuid,
    user_id: Uuid,
    kind: String,
    assignee: String,
    state: String,
    resources: Json<Value>,
    payload: Json<Value>,
    error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    // Node name of the API instance that owns this task, and half of the
    // row's address: `tasks` is partitioned on it.
    pub assignee: String,
    pub state: String,
    pub resources: Value,
    pub payload: Value,
    pub error: Option<String>,
    pub created_at: f64,
    pub updated_at: f64,
}

impl From<TaskRow> for TaskRecord {
    fn from(row: TaskRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            kind: row.kind,
            assignee: row.assignee,
            state: row.state,
            resources: row.resources.0,
            payload: row.payload.0,
            error: row.error,
            created_at: ts(row.created_at),
            updated_at: ts(row.updated_at),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerTypeMapping {
    pub series: String,
    pub provider: String,
    pub provider_server_type: String,
    pub cities: Vec<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ServerTypeMappingRow {
    series: String,
    provider: String,
    provider_server_type: String,
    cities: Json<Value>,
}

impl From<ServerTypeMappingRow> for ServerTypeMapping {
    fn from(row: ServerTypeMappingRow) -> Self {
        Self {
            series: row.series,
            provider: row.provider,
            provider_server_type: row.provider_server_type,
            cities: strings(&row.cities.0),
        }
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProviderResource {
    pub provider: String,
    pub kind: String,
    pub code: String,
    pub data: Json<Value>,
}

/// The fields a server row is written with.
pub struct NewServer<'a> {
    pub provider: &'a str,
    pub provider_server_id: &'a str,
    pub r#type: &'a str,
    pub name: &'a str,
    pub status: &'a str,
    pub public_ip4: Option<&'a str>,
    pub public_ip6: Option<&'a str>,
}

const SERVER_COLUMNS: &str = "id, user_id, provider, provider_server_id, type, name, status, \
                              public_ip4, public_ip6, created_at, updated_at";
const TASK_COLUMNS: &str =
    "id, user_id, kind, assignee, state, resources, payload, error, created_at, updated_at";

// Prefix used when a provider's server type doesn't have a mapping yet and
// one needs to be minted on the fly, e.g. "x9" or "y9".
fn series_prefix(provider: &str) -> String {
    match provider {
        "hetzner" => "x".to_string(),
        "digitalocean" => "y".to_string(),
        other => other.chars().take(1).collect(),
    }
}

/// Storage over the sharded databases (see `db`).
#[derive(Clone)]
pub struct Store {
    db: Arc<Db>,
}

impl Store {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    async fn pool_for_key(&self, table: &str, key: &str) -> Result<sqlx::PgPool> {
        let partition = self.db.partition_for_key(table, key).await?;
        self.db.pool(&partition).await
    }

    async fn pool_for_user(&self, user_id: &Uuid) -> Result<sqlx::PgPool> {
        let partition = self.db.partition_for_user(user_id).await?;
        self.db.pool(&partition).await
    }

    async fn pool_for_table(&self, table: &str) -> Result<sqlx::PgPool> {
        let partition = self.db.partition_for_table(table).await?;
        self.db.pool(&partition).await
    }

    /// The instance holding a task: hash the node name carrying it against
    /// the `tasks` map.
    async fn task_pool(&self, assignee: &str) -> Result<sqlx::PgPool> {
        self.pool_for_key(schema::TASKS, assignee).await
    }

    // -- OTP (partitioned on the phone number) ----------------------------

    pub async fn put_otp(
        &self,
        phone: &str,
        code: &str,
        expires_at: DateTime<Utc>,
        resend_after: DateTime<Utc>,
    ) -> Result<()> {
        let pool = self.pool_for_key(schema::OTPS, phone).await?;
        sqlx::query(
            "INSERT INTO otps (phone, code_hash, expires_at, resend_after, attempts, created_at)
             VALUES ($1, $2, $3, $4, 0, $5)
             ON CONFLICT (phone) DO UPDATE SET
                 code_hash = EXCLUDED.code_hash,
                 expires_at = EXCLUDED.expires_at,
                 resend_after = EXCLUDED.resend_after,
                 attempts = 0",
        )
        .bind(phone)
        .bind(hash(code))
        .bind(expires_at)
        .bind(resend_after)
        .bind(Utc::now())
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn get_otp(&self, phone: &str) -> Result<Option<OtpRecord>> {
        let pool = self.pool_for_key(schema::OTPS, phone).await?;
        let row: Option<(DateTime<Utc>, DateTime<Utc>, i32)> =
            sqlx::query_as("SELECT expires_at, resend_after, attempts FROM otps WHERE phone = $1")
                .bind(phone)
                .fetch_optional(&pool)
                .await?;
        Ok(row.map(|(expires_at, resend_after, attempts)| OtpRecord {
            expires_at: ts(expires_at),
            resend_after: ts(resend_after),
            attempts,
        }))
    }

    pub async fn check_otp_code(&self, phone: &str, code: &str) -> Result<bool> {
        let pool = self.pool_for_key(schema::OTPS, phone).await?;
        let stored: Option<String> =
            sqlx::query_scalar("SELECT code_hash FROM otps WHERE phone = $1")
                .bind(phone)
                .fetch_optional(&pool)
                .await?;
        Ok(stored.is_some_and(|stored| bool::from(stored.as_bytes().ct_eq(hash(code).as_bytes()))))
    }

    pub async fn increment_attempts(&self, phone: &str) -> Result<i32> {
        let pool = self.pool_for_key(schema::OTPS, phone).await?;
        let attempts: Option<i32> = sqlx::query_scalar(
            "UPDATE otps SET attempts = attempts + 1 WHERE phone = $1 RETURNING attempts",
        )
        .bind(phone)
        .fetch_optional(&pool)
        .await?;
        Ok(attempts.unwrap_or(0))
    }

    pub async fn clear_otp(&self, phone: &str) -> Result<()> {
        let pool = self.pool_for_key(schema::OTPS, phone).await?;
        sqlx::query("DELETE FROM otps WHERE phone = $1")
            .bind(phone)
            .execute(&pool)
            .await?;
        Ok(())
    }

    // -- Users (partitioned on the user id, found via the directory) -------

    /// Returns (user_id, partition).
    ///
    /// Two maps are read here, which is the whole point of naming the table
    /// on every lookup: the phone number resolves against
    /// `user_directory`'s map, and the user id it yields resolves against
    /// `users`'. The two are unrelated — a phone and the user it names are
    /// not co-located — so the directory entry is written on one instance
    /// and the user row on another.
    pub async fn get_or_create_user(&self, phone: &str) -> Result<(Uuid, Partition)> {
        let directory = self.pool_for_key(schema::USER_DIRECTORY, phone).await?;
        let existing: Option<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM user_directory WHERE phone = $1")
                .bind(phone)
                .fetch_optional(&directory)
                .await?;
        if let Some(user_id) = existing {
            return Ok((user_id, self.db.partition_for_user(&user_id).await?));
        }

        // Write the user row before the directory entry: a crash in between
        // leaves an unreferenced user row, which is inert, whereas the other
        // order would leave the directory pointing at a user that does not
        // exist and lock the phone number out for good.
        let user_id = Uuid::new_v4();
        let user_partition = self.db.partition_for_user(&user_id).await?;
        let users = self.db.pool(&user_partition).await?;
        sqlx::query("INSERT INTO users (id, phone, created_at) VALUES ($1, $2, $3)")
            .bind(user_id)
            .bind(phone)
            .bind(Utc::now())
            .execute(&users)
            .await?;

        let registered: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO user_directory (phone, user_id, created_at) VALUES ($1, $2, $3)
             ON CONFLICT (phone) DO NOTHING RETURNING user_id",
        )
        .bind(phone)
        .bind(user_id)
        .bind(Utc::now())
        .fetch_optional(&directory)
        .await?;
        if registered.is_some() {
            return Ok((user_id, user_partition));
        }

        // Another request registered this phone number first; theirs is the
        // user that exists as far as everyone else is concerned, so drop ours
        // rather than leaving two.
        let winner: Uuid =
            sqlx::query_scalar("SELECT user_id FROM user_directory WHERE phone = $1")
                .bind(phone)
                .fetch_one(&directory)
                .await?;
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&users)
            .await?;
        Ok((winner, self.db.partition_for_user(&winner).await?))
    }

    // -- API keys (partitioned on the key hash) ---------------------------
    // An arriving request has the token and nothing else, so the token's
    // hash is the whole address: `api_keys` is a root table with its own
    // map. The reverse direction — a user's own keys — goes through the
    // `user_api_keys` index, which lives with the user.

    pub async fn issue_api_key(&self, user_id: &Uuid, label: &str) -> Result<String> {
        let api_key = token_urlsafe();
        let key_hash = hash(&api_key);
        let now = Utc::now();
        let keys = self.pool_for_key(schema::API_KEYS, &key_hash).await?;
        sqlx::query(
            "INSERT INTO api_keys (key_hash, user_id, label, created_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(&key_hash)
        .bind(user_id)
        .bind(label)
        .bind(now)
        .execute(&keys)
        .await?;

        // Index second: a crash in between leaves a working key that does not
        // show up in listings, whereas the other order would list a key that
        // cannot be authenticated with and cannot be revoked either.
        let index = self.pool_for_user(user_id).await?;
        sqlx::query(
            "INSERT INTO user_api_keys (user_id, key_hash, created_at) VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(&key_hash)
        .bind(now)
        .execute(&index)
        .await?;
        Ok(api_key)
    }

    pub async fn get_api_key(&self, api_key: &str) -> Result<Option<ApiKeyRecord>> {
        let key_hash = hash(api_key);
        let keys = self.pool_for_key(schema::API_KEYS, &key_hash).await?;
        let row: Option<ApiKeyRow> = sqlx::query_as(
            "UPDATE api_keys SET last_used_at = $2
             WHERE key_hash = $1 AND revoked_at IS NULL
             RETURNING key_hash, user_id, label, created_at, last_used_at",
        )
        .bind(&key_hash)
        .bind(Utc::now())
        .fetch_optional(&keys)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };

        // The owner is on their own instance, which is also the one every
        // call made on their behalf will use.
        let partition = self.db.partition_for_user(&row.user_id).await?;
        let users = self.db.pool(&partition).await?;
        let phone: Option<String> = sqlx::query_scalar("SELECT phone FROM users WHERE id = $1")
            .bind(row.user_id)
            .fetch_optional(&users)
            .await?;
        Ok(phone.map(|phone| row.record(phone, partition)))
    }

    /// This user's live keys, read through the `user_api_keys` index.
    ///
    /// The index says *which* keys exist; each key's own row — label, last
    /// use, whether it is revoked — is on the instance its hash resolves to,
    /// so the hashes are grouped by instance and fetched one query per
    /// instance rather than one per key.
    pub async fn list_api_keys(&self, user_id: &Uuid) -> Result<Vec<ApiKeyRecord>> {
        let partition = self.db.partition_for_user(user_id).await?;
        let users = self.db.pool(&partition).await?;
        let hashes: Vec<String> =
            sqlx::query_scalar("SELECT key_hash FROM user_api_keys WHERE user_id = $1")
                .bind(user_id)
                .fetch_all(&users)
                .await?;
        let phone: Option<String> = sqlx::query_scalar("SELECT phone FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&users)
            .await?;
        let Some(phone) = phone else {
            return Ok(Vec::new());
        };

        let mut by_partition: Vec<(Partition, Vec<String>)> = Vec::new();
        for key_hash in hashes {
            let key_partition = self
                .db
                .partition_for_key(schema::API_KEYS, &key_hash)
                .await?;
            match by_partition.iter_mut().find(|(p, _)| *p == key_partition) {
                Some((_, group)) => group.push(key_hash),
                None => by_partition.push((key_partition, vec![key_hash])),
            }
        }

        let mut records = Vec::new();
        for (key_partition, group) in by_partition {
            let keys = self.db.pool(&key_partition).await?;
            let rows: Vec<ApiKeyRow> = sqlx::query_as(
                "SELECT key_hash, user_id, label, created_at, last_used_at FROM api_keys
                 WHERE key_hash = ANY($1) AND user_id = $2 AND revoked_at IS NULL",
            )
            .bind(&group)
            .bind(user_id)
            .fetch_all(&keys)
            .await?;
            records.extend(
                rows.into_iter()
                    .map(|row| row.record(phone.clone(), partition.clone())),
            );
        }
        records.sort_by(|a, b| b.created_at.total_cmp(&a.created_at));
        Ok(records)
    }

    pub async fn revoke_api_key(&self, api_key: &str) -> Result<()> {
        let key_hash = hash(api_key);
        let keys = self.pool_for_key(schema::API_KEYS, &key_hash).await?;
        sqlx::query(
            "UPDATE api_keys SET revoked_at = $2 WHERE key_hash = $1 AND revoked_at IS NULL",
        )
        .bind(&key_hash)
        .bind(Utc::now())
        .execute(&keys)
        .await?;
        Ok(())
    }

    /// Revoke one of a user's keys by its hash — the id the API hands out.
    /// The index row is what proves the key is theirs; without it a caller
    /// could revoke a hash belonging to someone else, since the `api_keys`
    /// row itself is on an instance that knows nothing about who is asking.
    pub async fn revoke_api_key_for_user(&self, user_id: &Uuid, key_hash: &str) -> Result<bool> {
        let users = self.pool_for_user(user_id).await?;
        let owned: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM user_api_keys WHERE user_id = $1 AND key_hash = $2")
                .bind(user_id)
                .bind(key_hash)
                .fetch_optional(&users)
                .await?;
        if owned.is_none() {
            return Ok(false);
        }
        let keys = self.pool_for_key(schema::API_KEYS, key_hash).await?;
        let revoked = sqlx::query(
            "UPDATE api_keys SET revoked_at = $2 WHERE key_hash = $1 AND revoked_at IS NULL",
        )
        .bind(key_hash)
        .bind(Utc::now())
        .execute(&keys)
        .await?;
        Ok(revoked.rows_affected() > 0)
    }

    // -- Web sessions (partitioned, routed by the token's shard prefix) ---

    pub async fn issue_web_session(&self, user_id: &Uuid, api_key_hash: &str) -> Result<String> {
        let token = mint_session_token(user_id);
        let users = self.pool_for_user(user_id).await?;
        sqlx::query(
            "INSERT INTO web_sessions (token_hash, user_id, api_key_hash, created_at)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(hash(&token))
        .bind(user_id)
        .bind(api_key_hash)
        .bind(Utc::now())
        .execute(&users)
        .await?;
        Ok(token)
    }

    /// The instance holding the `web_sessions` row for this token. The
    /// prefix is a bucket of the *owner's* `users` map, since that is what
    /// the row's placement follows.
    async fn session_partition(&self, token: &str) -> Result<Partition> {
        match session_token_bucket(token) {
            Some(bucket) => self.db.partition_for_bucket(schema::USERS, bucket).await,
            None => Ok(None),
        }
    }

    pub async fn get_web_session(&self, token: &str) -> Result<Option<WebSessionRecord>> {
        let partition = self.session_partition(token).await?;
        let pool = self.db.pool(&partition).await?;
        let row: Option<(Uuid, String, String, DateTime<Utc>)> = sqlx::query_as(
            "SELECT s.user_id, u.phone, s.api_key_hash, s.created_at
             FROM web_sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND s.revoked_at IS NULL",
        )
        .bind(hash(token))
        .fetch_optional(&pool)
        .await?;
        Ok(row.map(
            |(user_id, phone, api_key_hash, created_at)| WebSessionRecord {
                user_id,
                phone,
                partition,
                api_key_hash,
                created_at: ts(created_at),
            },
        ))
    }

    pub async fn revoke_web_session(&self, token: &str) -> Result<()> {
        let partition = self.session_partition(token).await?;
        let pool = self.db.pool(&partition).await?;
        sqlx::query("UPDATE web_sessions SET revoked_at = $2 WHERE token_hash = $1")
            .bind(hash(token))
            .bind(Utc::now())
            .execute(&pool)
            .await?;
        Ok(())
    }

    // -- Provider credentials (partitioned) -------------------------------
    // A user's provider secrets live with the rest of their rows, on the
    // instance their user id hashes to — which is also where the `users`
    // row the foreign key points at is.

    pub async fn set_provider_credentials(
        &self,
        partition: &Partition,
        user_id: &Uuid,
        provider: &str,
        credentials_encrypted: &str,
    ) -> Result<()> {
        let pool = self.db.pool(partition).await?;
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO provider_credentials
                 (user_id, provider, credentials_encrypted, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $4)
             ON CONFLICT (user_id, provider) DO UPDATE SET
                 credentials_encrypted = EXCLUDED.credentials_encrypted,
                 updated_at = EXCLUDED.updated_at",
        )
        .bind(user_id)
        .bind(provider)
        .bind(credentials_encrypted)
        .bind(now)
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn get_provider_credentials(
        &self,
        partition: &Partition,
        user_id: &Uuid,
        provider: &str,
    ) -> Result<Option<String>> {
        let pool = self.db.pool(partition).await?;
        Ok(sqlx::query_scalar(
            "SELECT credentials_encrypted FROM provider_credentials
             WHERE user_id = $1 AND provider = $2",
        )
        .bind(user_id)
        .bind(provider)
        .fetch_optional(&pool)
        .await?)
    }

    pub async fn delete_provider_credentials(
        &self,
        partition: &Partition,
        user_id: &Uuid,
        provider: &str,
    ) -> Result<bool> {
        let pool = self.db.pool(partition).await?;
        let deleted =
            sqlx::query("DELETE FROM provider_credentials WHERE user_id = $1 AND provider = $2")
                .bind(user_id)
                .bind(provider)
                .execute(&pool)
                .await?;
        Ok(deleted.rows_affected() > 0)
    }

    pub async fn list_configured_providers(
        &self,
        partition: &Partition,
        user_id: &Uuid,
    ) -> Result<Vec<String>> {
        let pool = self.db.pool(partition).await?;
        Ok(
            sqlx::query_scalar("SELECT provider FROM provider_credentials WHERE user_id = $1")
                .bind(user_id)
                .fetch_all(&pool)
                .await?,
        )
    }

    // -- Server type mappings (one whole-space range; see db) -------------

    pub async fn get_server_type_mapping(&self, series: &str) -> Result<Option<ServerTypeMapping>> {
        let pool = self.pool_for_table(schema::SERVER_TYPE_MAPPINGS).await?;
        let row: Option<ServerTypeMappingRow> = sqlx::query_as(
            "SELECT series, provider, provider_server_type, cities
             FROM server_type_mappings WHERE series = $1",
        )
        .bind(series)
        .fetch_optional(&pool)
        .await?;
        Ok(row.map(Into::into))
    }

    /// Look up the series for a provider's raw server type, minting a new
    /// one (and persisting it) if this type has never been seen before.
    /// Guarantees every server type we ever expose is one of our own
    /// "{series}-{city}" identifiers, never the provider's own name.
    ///
    /// `cities` (our own city codes this type is currently known to be
    /// available in) is merged into the series' stored city list — added
    /// to, never removed from, so a provider momentarily omitting a city
    /// from one response doesn't erase it.
    pub async fn get_or_create_series_for_provider_type(
        &self,
        provider: &str,
        provider_server_type: &str,
        cities: &[String],
    ) -> Result<String> {
        let pool = self.pool_for_table(schema::SERVER_TYPE_MAPPINGS).await?;
        let existing: Option<ServerTypeMappingRow> = sqlx::query_as(
            "SELECT series, provider, provider_server_type, cities FROM server_type_mappings
             WHERE provider = $1 AND provider_server_type = $2 LIMIT 1",
        )
        .bind(provider)
        .bind(provider_server_type)
        .fetch_optional(&pool)
        .await?;
        if let Some(mapping) = existing.map(ServerTypeMapping::from) {
            if !cities.is_empty() {
                let merged: Vec<String> = mapping
                    .cities
                    .iter()
                    .chain(cities)
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if merged != mapping.cities {
                    sqlx::query(
                        "UPDATE server_type_mappings SET cities = $2::json WHERE series = $1",
                    )
                    .bind(&mapping.series)
                    .bind(Value::from(merged).to_string())
                    .execute(&pool)
                    .await?;
                }
            }
            return Ok(mapping.series);
        }

        let prefix = series_prefix(provider);
        let known: Vec<String> =
            sqlx::query_scalar("SELECT series FROM server_type_mappings WHERE provider = $1")
                .bind(provider)
                .fetch_all(&pool)
                .await?;
        let next = known
            .iter()
            .filter_map(|series| series.strip_prefix(&prefix))
            .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
            .filter_map(|n| n.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        let series = format!("{prefix}{next}");
        let cities: BTreeSet<&String> = cities.iter().collect();
        sqlx::query(
            "INSERT INTO server_type_mappings (series, provider, provider_server_type, cities)
             VALUES ($1, $2, $3, $4::json)",
        )
        .bind(&series)
        .bind(provider)
        .bind(provider_server_type)
        .bind(serde_json::to_string(&cities)?)
        .execute(&pool)
        .await?;
        Ok(series)
    }

    pub async fn get_server_type_cities(&self, series: &str) -> Result<Vec<String>> {
        Ok(self
            .get_server_type_mapping(series)
            .await?
            .map(|mapping| mapping.cities)
            .unwrap_or_default())
    }

    // -- Location mappings (one whole-space range; see db) ----------------

    pub async fn get_location_mapping(&self, code: &str, provider: &str) -> Result<Option<String>> {
        let pool = self.pool_for_table(schema::LOCATION_MAPPINGS).await?;
        Ok(sqlx::query_scalar(
            "SELECT provider_location_code FROM location_mappings WHERE code = $1 AND provider = $2",
        )
        .bind(code)
        .bind(provider)
        .fetch_optional(&pool)
        .await?)
    }

    /// Record our (code, provider) -> provider location mapping if it isn't
    /// already known. Never overwrites an existing mapping, so once minted
    /// it's fixed even if the provider's own data shifts later.
    pub async fn set_location_mapping(
        &self,
        code: &str,
        provider: &str,
        provider_location_code: &str,
    ) -> Result<()> {
        let pool = self.pool_for_table(schema::LOCATION_MAPPINGS).await?;
        sqlx::query(
            "INSERT INTO location_mappings (code, provider, provider_location_code)
             VALUES ($1, $2, $3) ON CONFLICT (code, provider) DO NOTHING",
        )
        .bind(code)
        .bind(provider)
        .bind(provider_location_code)
        .execute(&pool)
        .await?;
        Ok(())
    }

    // -- Provider catalog (one whole-space range; see db) ------------------
    // Raw locations/server-types/images mirrored from each provider by
    // POST /servers/sync, used to answer "what's available" without hitting
    // the provider's API live every time.

    pub async fn upsert_provider_resource(
        &self,
        provider: &str,
        kind: &str,
        code: &str,
        data: &Value,
    ) -> Result<()> {
        let pool = self.pool_for_table(schema::PROVIDER_RESOURCES).await?;
        sqlx::query(
            "INSERT INTO provider_resources (id, provider, kind, code, data, updated_at)
             VALUES ($1, $2, $3, $4, $5::json, $6)
             ON CONFLICT (provider, kind, code) DO UPDATE SET
                 data = EXCLUDED.data, updated_at = EXCLUDED.updated_at",
        )
        .bind(Uuid::new_v4())
        .bind(provider)
        .bind(kind)
        .bind(code)
        .bind(data.to_string())
        .bind(Utc::now())
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn list_provider_resources(
        &self,
        provider: Option<&str>,
        kind: Option<&str>,
    ) -> Result<Vec<ProviderResource>> {
        let pool = self.pool_for_table(schema::PROVIDER_RESOURCES).await?;
        Ok(sqlx::query_as(
            "SELECT provider, kind, code, data FROM provider_resources
             WHERE ($1::text IS NULL OR provider = $1) AND ($2::text IS NULL OR kind = $2)
             ORDER BY code",
        )
        .bind(provider)
        .bind(kind)
        .fetch_all(&pool)
        .await?)
    }

    // -- Servers (partitioned) ---------------------------------------------
    // A server row lives on its owner's instance, so nothing here can be
    // reached without the owner's partition.

    pub async fn create_server(
        &self,
        partition: &Partition,
        user_id: &Uuid,
        server: NewServer<'_>,
    ) -> Result<ServerRow> {
        let pool = self.db.pool(partition).await?;
        let now = Utc::now();
        Ok(sqlx::query_as(&format!(
            "INSERT INTO servers (id, user_id, provider, provider_server_id, type, name, status,
                                  public_ip4, public_ip6, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
             RETURNING {SERVER_COLUMNS}"
        ))
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(server.provider)
        .bind(server.provider_server_id)
        .bind(server.r#type)
        .bind(server.name)
        .bind(server.status)
        .bind(server.public_ip4)
        .bind(server.public_ip6)
        .bind(now)
        .fetch_one(&pool)
        .await?)
    }

    pub async fn get_server(
        &self,
        partition: &Partition,
        user_id: &Uuid,
        server_id: &str,
    ) -> Result<Option<ServerRow>> {
        let Ok(server_id) = Uuid::parse_str(server_id) else {
            return Ok(None);
        };
        let pool = self.db.pool(partition).await?;
        Ok(sqlx::query_as(&format!(
            "SELECT {SERVER_COLUMNS} FROM servers WHERE id = $1 AND user_id = $2"
        ))
        .bind(server_id)
        .bind(user_id)
        .fetch_optional(&pool)
        .await?)
    }

    pub async fn list_servers(
        &self,
        partition: &Partition,
        user_id: &Uuid,
    ) -> Result<Vec<ServerRow>> {
        let pool = self.db.pool(partition).await?;
        Ok(sqlx::query_as(&format!(
            "SELECT {SERVER_COLUMNS} FROM servers WHERE user_id = $1 ORDER BY created_at DESC"
        ))
        .bind(user_id)
        .fetch_all(&pool)
        .await?)
    }

    pub async fn delete_server(
        &self,
        partition: &Partition,
        user_id: &Uuid,
        server_id: &str,
    ) -> Result<Option<ServerRow>> {
        let Ok(server_id) = Uuid::parse_str(server_id) else {
            return Ok(None);
        };
        let pool = self.db.pool(partition).await?;
        Ok(sqlx::query_as(&format!(
            "DELETE FROM servers WHERE id = $1 AND user_id = $2 RETURNING {SERVER_COLUMNS}"
        ))
        .bind(server_id)
        .bind(user_id)
        .fetch_optional(&pool)
        .await?)
    }

    pub async fn set_server_status(
        &self,
        partition: &Partition,
        user_id: &Uuid,
        server_id: &str,
        status: &str,
    ) -> Result<Option<ServerRow>> {
        let Ok(server_id) = Uuid::parse_str(server_id) else {
            return Ok(None);
        };
        let pool = self.db.pool(partition).await?;
        Ok(sqlx::query_as(&format!(
            "UPDATE servers SET status = $3, updated_at = $4 WHERE id = $1 AND user_id = $2
             RETURNING {SERVER_COLUMNS}"
        ))
        .bind(server_id)
        .bind(user_id)
        .bind(status)
        .bind(Utc::now())
        .fetch_optional(&pool)
        .await?)
    }

    /// Insert or update a server matched by (provider, provider_server_id).
    /// Returns (row, created).
    pub async fn upsert_server_by_provider_id(
        &self,
        partition: &Partition,
        user_id: &Uuid,
        server: NewServer<'_>,
    ) -> Result<(ServerRow, bool)> {
        let pool = self.db.pool(partition).await?;
        let updated: Option<ServerRow> = sqlx::query_as(&format!(
            "UPDATE servers SET type = $4, name = $5, status = $6, public_ip4 = $7,
                                public_ip6 = $8, updated_at = $9
             WHERE user_id = $1 AND provider = $2 AND provider_server_id = $3
             RETURNING {SERVER_COLUMNS}"
        ))
        .bind(user_id)
        .bind(server.provider)
        .bind(server.provider_server_id)
        .bind(server.r#type)
        .bind(server.name)
        .bind(server.status)
        .bind(server.public_ip4)
        .bind(server.public_ip6)
        .bind(Utc::now())
        .fetch_optional(&pool)
        .await?;
        match updated {
            Some(row) => Ok((row, false)),
            None => Ok((self.create_server(partition, user_id, server).await?, true)),
        }
    }

    // -- Tasks (partitioned on the assignee) --------------------------------
    // A task is a suspended, resumable workflow acting on provider-owned
    // resources. It is a root table hashed on `assignee`, the name of the
    // API instance that picked it up: only the assignee advances a task, and
    // it does so by sweeping — so "this node's tasks" is the read that
    // decides placement, and it lands on one instance.
    //
    // A task therefore does *not* sit with the user rows its steps mutate;
    // those are reached through `Db::partition_for_user(task.user_id)`. The
    // reverse question — a user's own in-flight tasks — goes through the
    // `user_tasks` index, which does live with the user.

    /// Create a task owned by `assignee`, on the instance that node's name
    /// hashes to, and index it under its user.
    pub async fn create_task(
        &self,
        user_id: &Uuid,
        kind: &str,
        assignee: &str,
        state: &str,
        resources: &Value,
        payload: &Value,
    ) -> Result<TaskRecord> {
        let pool = self.task_pool(assignee).await?;
        let now = Utc::now();
        let row: TaskRow = sqlx::query_as(&format!(
            "INSERT INTO tasks (id, user_id, kind, assignee, state, resources, payload,
                                created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6::json, $7::json, $8, $8)
             RETURNING {TASK_COLUMNS}"
        ))
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(kind)
        .bind(assignee)
        .bind(state)
        .bind(resources.to_string())
        .bind(payload.to_string())
        .bind(now)
        .fetch_one(&pool)
        .await?;

        // Index second, as for API keys: a crash in between leaves a task
        // that still runs to completion but doesn't show up in GET /tasks,
        // whereas the other order would list a task that does not exist.
        let index = self.pool_for_user(user_id).await?;
        sqlx::query(
            "INSERT INTO user_tasks (user_id, task_id, assignee, created_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(row.id)
        .bind(assignee)
        .bind(now)
        .execute(&index)
        .await?;
        Ok(row.into())
    }

    /// One task, addressed the only way a task can be: by the node holding
    /// it and its id.
    pub async fn get_task(&self, assignee: &str, task_id: &Uuid) -> Result<Option<TaskRecord>> {
        let pool = self.task_pool(assignee).await?;
        let row: Option<TaskRow> = sqlx::query_as(&format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE assignee = $1 AND id = $2"
        ))
        .bind(assignee)
        .bind(task_id)
        .fetch_optional(&pool)
        .await?;
        Ok(row.map(Into::into))
    }

    /// Everything `assignee` is carrying — what the sweep runs on. One query
    /// on one instance, since that is what the table is hashed for.
    pub async fn list_tasks(&self, assignee: &str) -> Result<Vec<TaskRecord>> {
        let pool = self.task_pool(assignee).await?;
        let rows: Vec<TaskRow> = sqlx::query_as(&format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE assignee = $1 ORDER BY created_at"
        ))
        .bind(assignee)
        .fetch_all(&pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// This user's in-flight tasks, read through the `user_tasks` index.
    ///
    /// The index says which tasks exist and which node holds each one; the
    /// rows themselves are on the instances those node names hash to, so
    /// they are grouped by assignee and fetched one query per assignee.
    pub async fn list_user_tasks(&self, user_id: &Uuid) -> Result<Vec<TaskRecord>> {
        let index = self.pool_for_user(user_id).await?;
        let indexed: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT task_id, assignee FROM user_tasks WHERE user_id = $1")
                .bind(user_id)
                .fetch_all(&index)
                .await?;

        let mut by_assignee: HashMap<String, Vec<Uuid>> = HashMap::new();
        for (task_id, assignee) in indexed {
            by_assignee.entry(assignee).or_default().push(task_id);
        }

        let mut records: Vec<TaskRecord> = Vec::new();
        for (assignee, task_ids) in by_assignee {
            let pool = self.task_pool(&assignee).await?;
            let rows: Vec<TaskRow> = sqlx::query_as(&format!(
                "SELECT {TASK_COLUMNS} FROM tasks
                 WHERE assignee = $1 AND id = ANY($2) AND user_id = $3"
            ))
            .bind(&assignee)
            .bind(&task_ids)
            .bind(user_id)
            .fetch_all(&pool)
            .await?;
            records.extend(rows.into_iter().map(TaskRecord::from));
        }
        records.sort_by(|a, b| a.created_at.total_cmp(&b.created_at));
        Ok(records)
    }

    /// One of this user's tasks by id. The index row is what proves the task
    /// is theirs *and* what says which node to read it from — a task id
    /// alone addresses nothing.
    pub async fn get_user_task(&self, user_id: &Uuid, task_id: &str) -> Result<Option<TaskRecord>> {
        let Ok(task_id) = Uuid::parse_str(task_id) else {
            return Ok(None);
        };
        let index = self.pool_for_user(user_id).await?;
        let assignee: Option<String> = sqlx::query_scalar(
            "SELECT assignee FROM user_tasks WHERE user_id = $1 AND task_id = $2",
        )
        .bind(user_id)
        .bind(task_id)
        .fetch_optional(&index)
        .await?;
        let Some(assignee) = assignee else {
            return Ok(None);
        };
        let task = self.get_task(&assignee, &task_id).await?;
        Ok(task.filter(|task| task.user_id == *user_id))
    }

    /// Move a task to `state`, returning whether the write happened.
    ///
    /// `expected_state` makes this a compare-and-set: the write is skipped if
    /// the task has moved on since the caller read it. Two runs of the same
    /// step can overlap (the request path advances a task inline while the
    /// sweep picks up the same row), and without this the loser's write —
    /// in particular the error path, which rewrites the state it started
    /// from — silently undoes the winner's progress.
    pub async fn set_task_state(
        &self,
        assignee: &str,
        task_id: &Uuid,
        state: &str,
        payload: Option<&Value>,
        error: Option<&str>,
        expected_state: Option<&str>,
    ) -> Result<bool> {
        let pool = self.task_pool(assignee).await?;
        let updated = sqlx::query(
            "UPDATE tasks SET state = $3, payload = COALESCE($4::json, payload), error = $5,
                              updated_at = $6
             WHERE assignee = $1 AND id = $2 AND ($7::text IS NULL OR state = $7)",
        )
        .bind(assignee)
        .bind(task_id)
        .bind(state)
        .bind(payload.map(Value::to_string))
        .bind(error)
        .bind(Utc::now())
        .bind(expected_state)
        .execute(&pool)
        .await?;
        Ok(updated.rows_affected() > 0)
    }

    /// Remove a finished task and its index entry. The index goes first: an
    /// entry pointing at a task that is gone would be reported as in-flight
    /// forever, while a task with no entry is merely invisible to GET /tasks
    /// for the moment it takes to delete it.
    pub async fn delete_task(&self, assignee: &str, task_id: &Uuid) -> Result<()> {
        let Some(task) = self.get_task(assignee, task_id).await? else {
            return Ok(());
        };
        let index = self.pool_for_user(&task.user_id).await?;
        sqlx::query("DELETE FROM user_tasks WHERE user_id = $1 AND task_id = $2")
            .bind(task.user_id)
            .bind(task_id)
            .execute(&index)
            .await?;
        let pool = self.task_pool(assignee).await?;
        sqlx::query("DELETE FROM tasks WHERE assignee = $1 AND id = $2")
            .bind(assignee)
            .bind(task_id)
            .execute(&pool)
            .await
            .context("deleting a finished task")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_tokens_carry_the_owners_bucket() {
        let user = Uuid::parse_str("3f2504e0-4f89-11d3-9a0c-0305e82c3301").unwrap();
        let token = mint_session_token(&user);
        assert!(token.starts_with("542b."), "{token}"); // 21547 == 0x542b
        assert_eq!(session_token_bucket(&token), Some(21547));
        assert_eq!(session_token_bucket("legacy-token-without-prefix"), None);
        assert_eq!(session_token_bucket("zzzz.secret"), None);
        assert_eq!(session_token_bucket("12345.secret"), None);
    }

    #[test]
    fn tokens_and_hashes_look_like_pythons() {
        assert_eq!(token_urlsafe().len(), 43);
        assert_eq!(
            hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
