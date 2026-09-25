// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Shard-aware database access.
//!
//! usecode agent runs more than one PostgreSQL instance. Every instance
//! carries the **identical schema**; what differs is which rows live where.
//!
//! Placement is computed, not stored, and **every table is partitioned**
//! (see [`crate::schema`]). A table with no foreign key is partitioned on
//! its own key; a table with a foreign key inherits the placement of the row
//! its first foreign key points at. So only the first kind needs a map:
//!
//! ```text
//! bucket = virtual_shard(key)              // pure function, no I/O
//! ```
//!
//! The main database's `shard_ranges` table maps a contiguous run of buckets
//! to the physical instance holding them, **per table**: every row names the
//! table whose map it belongs to, and every lookup names the table it is
//! reading. The whole address of a user-owned row is therefore its owner's
//! id — hash it against the `users` map — and the whole address of a
//! directory entry is the phone number hashed against the `user_directory`
//! map. Because a user and everything keyed by their id land on the same
//! instance, those child tables carry **real foreign keys to** `users`.
//!
//! The instance a bucket resolves to is its **partition key**:
//!
//! - `None` means the **main** (first) database — the one
//!   `USECODE_AGENT_DATABASE_URL` points at.
//! - Any other partition key names one of the `USECODE_AGENT_SHARDS`
//!   instances.
//!
//! Consequently **no user-owned row can be reached without an owning user
//! id**: [`Db::pool`] is the only way to get a connection, it takes the
//! partition key as its argument, and [`Db::partition_for_key`] (or its
//! `users` shorthand [`Db::partition_for_user`]) is the only way to compute
//! one. There is no "try every shard" path for row data — the only place
//! that walks every database is migration, which reads static configuration
//! ([`configured_urls`]).
//!
//! `shard_ranges` is seeded at startup from the `USECODE_AGENT_SHARDS`
//! setting so a fresh deployment can bootstrap, but runtime resolution
//! always reads the table. Ranges are never re-pointed once seeded: the
//! buckets in a range are physically on that instance, so handing a range to
//! a different instance means physically moving those rows.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::RwLock;

use anyhow::{Result, bail};
use blake2::Blake2bVar;
use blake2::digest::{Update, VariableOutput};
use chrono::Utc;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use crate::config::Settings;
use crate::schema;

/// How many virtual shards every key space is hashed into. Fixed forever:
/// changing it would re-address every existing row. It is deliberately far
/// larger than any plausible number of database instances, so growing the
/// deployment is a matter of splitting bucket ranges rather than re-hashing.
pub const VIRTUAL_SHARDS: u32 = 65536;

/// PostgreSQL can't hold NULL in a primary key, so the main shard's rows in
/// `shard_ranges` use the empty string as the on-disk spelling of "the null
/// partition key". Nothing outside this module sees the sentinel.
const MAIN_PARTITION_ROW_KEY: &str = "";

// Advisory lock id held while seeding `shard_ranges`; distinct from the
// migration lock, and the same in every instance.
const SEED_LOCK_ID: i64 = 0x7245_5341_4E41_4B01;

/// Which database instance a row is on; `None` is the main one.
pub type Partition = Option<String>;

// -- Virtual shards ------------------------------------------------------

/// Which of the [`VIRTUAL_SHARDS`] buckets a key's row belongs to.
///
/// A fixed hash of the key — BLAKE2b truncated to the bucket width — so
/// every instance and every process computes the same answer with no
/// coordination and no stored mapping.
///
/// Text that parses as a UUID hashes over its 16 raw bytes and anything else
/// over its UTF-8 text. The UUID case is spelled out because it is the
/// placement of every user that already exists: hashing a user id's
/// *string* would put it in a different bucket and re-address every row in
/// the deployment.
pub fn virtual_shard(key: &str) -> u32 {
    match Uuid::parse_str(key) {
        Ok(id) => virtual_shard_bytes(id.as_bytes()),
        Err(_) => virtual_shard_bytes(key.as_bytes()),
    }
}

pub fn virtual_shard_uuid(key: &Uuid) -> u32 {
    virtual_shard_bytes(key.as_bytes())
}

fn virtual_shard_bytes(raw: &[u8]) -> u32 {
    let mut hasher = Blake2bVar::new(2).expect("2 is a valid BLAKE2b digest size");
    hasher.update(raw);
    let mut digest = [0u8; 2];
    hasher
        .finalize_variable(&mut digest)
        .expect("digest buffer matches the configured size");
    u32::from(u16::from_be_bytes(digest)) % VIRTUAL_SHARDS
}

// -- URL plumbing --------------------------------------------------------
// Shards are configured as bare "host:port/database" targets rather than
// full DSNs, because every instance runs the identical schema under the
// identical role — the driver and credentials come from the main URL, so
// there is only one place to change them.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: i32,
    pub database: String,
}

fn split_target(target: &str) -> Target {
    let (host_port, database) = target.split_once('/').unwrap_or((target, ""));
    let (host, port) = host_port.split_once(':').unwrap_or((host_port, ""));
    Target {
        host: host.to_string(),
        port: port.parse().unwrap_or(5432),
        database: database.to_string(),
    }
}

/// A DSN split the way the rest of this module needs it.
struct Dsn<'a> {
    scheme: &'a str,
    userinfo: Option<&'a str>,
    host: &'a str,
    port: Option<i32>,
    path: &'a str,
    rest: &'a str, // "?query#fragment", verbatim
}

fn parse_dsn(url: &str) -> Dsn<'_> {
    let (scheme, after) = url.split_once("://").unwrap_or(("postgresql", url));
    let netloc_end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let (netloc, tail) = after.split_at(netloc_end);
    let path_end = tail.find(['?', '#']).unwrap_or(tail.len());
    let (path, rest) = tail.split_at(path_end);
    let (userinfo, hostport) = match netloc.rsplit_once('@') {
        Some((userinfo, hostport)) => (Some(userinfo), hostport),
        None => (None, netloc),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((host, port)) if !port.contains(']') => (host, port.parse().ok()),
        _ => (hostport, None),
    };
    Dsn {
        scheme,
        userinfo,
        host,
        port,
        path,
        rest,
    }
}

/// The main database's (host, port, database), read off its DSN.
pub fn main_target(settings: &Settings) -> Target {
    let dsn = parse_dsn(&settings.database_url);
    Target {
        host: if dsn.host.is_empty() {
            "localhost".to_string()
        } else {
            dsn.host.to_ascii_lowercase()
        },
        port: dsn.port.unwrap_or(5432),
        database: dsn.path.trim_start_matches('/').to_string(),
    }
}

/// Rebuild the main DSN — same driver, same credentials — pointed at a
/// different instance.
pub fn shard_url(settings: &Settings, target: &Target) -> String {
    let dsn = parse_dsn(&settings.database_url);
    let userinfo = dsn.userinfo.map(|u| format!("{u}@")).unwrap_or_default();
    format!(
        "{}://{userinfo}{}:{}/{}{}",
        dsn.scheme, target.host, target.port, target.database, dsn.rest
    )
}

/// The URL as the PostgreSQL driver wants it: SQLAlchemy's
/// `postgresql+asyncpg://` spelling is still accepted in configuration.
pub fn driver_url(url: &str) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let base = scheme.split('+').next().unwrap_or(scheme);
            format!("{base}://{rest}")
        }
        None => url.to_string(),
    }
}

/// Every database this deployment owns, keyed by partition key (`None` for
/// the main one), taken from *static configuration* rather than from the
/// range table. Used for the two things that must work before the table can
/// be read: applying migrations, and seeding the table itself.
pub fn configured_urls(settings: &Settings) -> Vec<(Partition, String)> {
    let mut urls = vec![(None, settings.database_url.clone())];
    for (key, target) in &settings.shards {
        if key.is_empty() {
            continue; // the main shard is `database_url`, not a shards entry
        }
        urls.push((
            Some(key.clone()),
            shard_url(settings, &split_target(target)),
        ));
    }
    urls
}

fn lazy_pool(url: &str) -> Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(10)
        .connect_lazy(&driver_url(url))?)
}

// -- Shard map -----------------------------------------------------------

struct Shard {
    url: String,
    pool: PgPool,
}

/// One table's map: `starts[i]` is the first bucket owned by `owners[i]`. A
/// bucket is resolved with one binary search.
#[derive(Clone, Default)]
struct Ranges {
    starts: Vec<u32>,
    owners: Vec<Partition>,
}

#[derive(Default)]
struct ShardMap {
    // Partition key -> shard, for every non-main partition.
    shards: HashMap<String, Shard>,
    ranges: HashMap<String, Ranges>,
}

pub struct Db {
    settings: Settings,
    main: PgPool,
    map: RwLock<ShardMap>,
}

#[derive(sqlx::FromRow)]
struct ShardRangeRow {
    table: String,
    start_bucket: i32,
    partition_key: String,
    host: String,
    port: i32,
    database: String,
}

/// Split the bucket space into `count` contiguous, near-equal ranges.
fn even_ranges(count: u32) -> Vec<(u32, u32)> {
    let (size, remainder) = (VIRTUAL_SHARDS / count, VIRTUAL_SHARDS % count);
    let mut start = 0;
    (0..count)
        .map(|index| {
            let width = size + u32::from(index < remainder);
            let range = (start, start + width - 1);
            start += width;
            range
        })
        .collect()
}

impl Db {
    /// Connect to the main database, check the declared placements against
    /// its schema, and load the shard map.
    pub async fn connect(settings: &Settings) -> Result<Self> {
        let main = lazy_pool(&settings.database_url)?;
        schema::check_placements(&main).await?;
        let db = Self {
            settings: settings.clone(),
            main,
            map: RwLock::new(ShardMap::default()),
        };
        db.refresh_shard_map().await?;
        Ok(db)
    }

    pub async fn close(&self) {
        let shards: Vec<PgPool> = {
            let mut map = self.map.write().unwrap();
            map.ranges.clear();
            map.shards.drain().map(|(_, shard)| shard.pool).collect()
        };
        for pool in shards {
            pool.close().await;
        }
        self.main.close().await;
    }

    /// Every instance this deployment is configured with, main first, as
    /// (partition row key, target).
    fn instance_targets(&self) -> Vec<(String, Target)> {
        let mut targets = vec![(
            MAIN_PARTITION_ROW_KEY.to_string(),
            main_target(&self.settings),
        )];
        let sorted: BTreeMap<_, _> = self.settings.shards.iter().collect();
        for (key, target) in sorted {
            if key.is_empty() {
                continue; // the main shard is `database_url`, not a shards entry
            }
            targets.push((key.clone(), split_target(target)));
        }
        targets
    }

    /// Record this deployment's bucket-to-instance topology in the main
    /// database's `shard_ranges` table, so placement resolves from the
    /// database rather than from whatever configuration each instance
    /// happens to hold.
    ///
    /// One map per table in [`schema::mapped_tables`], seeded once each: a
    /// table that already has rows here is left alone, since the buckets in
    /// a range are physically on that instance and re-pointing one would
    /// strand them. Seeding per table rather than all-or-nothing is what
    /// lets a table added later get its map on the next boot without
    /// disturbing the tables that already have one.
    ///
    /// A whole-space table gets a single range on the main database. So does
    /// a table that *already holds rows* on the main database in a
    /// deployment that has other instances — those rows are physically
    /// here, and this function only writes the map, it cannot move them.
    pub async fn seed_shard_ranges(&self) -> Result<()> {
        let targets = self.instance_targets();
        let mut tx = self.main.begin().await?;
        // Instances booting together would otherwise both find a table
        // unmapped and race to seed it; the losers wait here and then find
        // the map already written.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(SEED_LOCK_ID)
            .execute(&mut *tx)
            .await?;
        let already: HashSet<String> =
            sqlx::query_scalar(r#"SELECT DISTINCT "table" FROM shard_ranges"#)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .collect();

        for table in schema::mapped_tables() {
            if already.contains(table) {
                continue;
            }
            let mut table_targets = &targets[..];
            if schema::is_whole_space(table) {
                table_targets = &targets[..1];
            } else if targets.len() > 1 {
                // `table` comes from the static schema declarations, never
                // from input.
                let existing: i64 =
                    sqlx::query_scalar(&format!(r#"SELECT count(*) FROM "{table}""#))
                        .fetch_one(&mut *tx)
                        .await?;
                if existing > 0 {
                    let others: Vec<&str> = targets[1..].iter().map(|(k, _)| k.as_str()).collect();
                    tracing::warn!(
                        "Seeding all {VIRTUAL_SHARDS} virtual shards of {table} on the main \
                         database: it already holds {existing} row(s), which are physically \
                         there. Give {} a bucket range only by splitting one and moving those rows.",
                        others.join(", ")
                    );
                    table_targets = &targets[..1];
                }
            }

            for ((partition_key, target), (start, end)) in table_targets
                .iter()
                .zip(even_ranges(table_targets.len() as u32))
            {
                sqlx::query(
                    r#"INSERT INTO shard_ranges
                         ("table", start_bucket, end_bucket, partition_key, host, port, database, created_at)
                       VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                )
                .bind(table)
                .bind(start as i32)
                .bind(end as i32)
                .bind(partition_key)
                .bind(&target.host)
                .bind(target.port)
                .bind(&target.database)
                .bind(Utc::now())
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        self.refresh_shard_map().await
    }

    /// Reload every table's bucket-range map from the main database.
    pub async fn refresh_shard_map(&self) -> Result<()> {
        let rows: Vec<ShardRangeRow> = sqlx::query_as(
            r#"SELECT "table", start_bucket, partition_key, host, port, database
               FROM shard_ranges ORDER BY "table", start_bucket"#,
        )
        .fetch_all(&self.main)
        .await?;

        let mut ranges: HashMap<String, Ranges> = HashMap::new();
        let mut urls: HashMap<String, String> = HashMap::new();
        for row in rows {
            let entry = ranges.entry(row.table).or_default();
            entry.starts.push(row.start_bucket as u32);
            if row.partition_key == MAIN_PARTITION_ROW_KEY {
                entry.owners.push(None); // the null partition is the main shard
                continue;
            }
            entry.owners.push(Some(row.partition_key.clone()));
            let target = Target {
                host: row.host,
                port: row.port,
                database: row.database,
            };
            urls.insert(row.partition_key, shard_url(&self.settings, &target));
        }

        let stale: Vec<PgPool> = {
            let mut map = self.map.write().unwrap();
            for (key, url) in &urls {
                let current = map.shards.get(key).map(|shard| shard.url.as_str());
                if current != Some(url.as_str()) {
                    let shard = Shard {
                        url: url.clone(),
                        pool: lazy_pool(url)?,
                    };
                    map.shards.insert(key.clone(), shard);
                }
            }
            let gone: Vec<String> = map
                .shards
                .keys()
                .filter(|key| !urls.contains_key(*key))
                .cloned()
                .collect();
            map.ranges = ranges;
            gone.into_iter()
                .filter_map(|key| map.shards.remove(&key).map(|shard| shard.pool))
                .collect()
        };
        for pool in stale {
            pool.close().await;
        }
        Ok(())
    }

    fn cached_ranges(&self, table: &str) -> Option<Ranges> {
        self.map.read().unwrap().ranges.get(table).cloned()
    }

    /// One table's map, re-reading it once if this process hasn't seen it.
    /// A table added since the last refresh — or since this process booted
    /// — is the ordinary reason for a miss.
    async fn table_ranges(&self, table: &str) -> Result<Ranges> {
        if let Some(found) = self.cached_ranges(table) {
            return Ok(found);
        }
        self.refresh_shard_map().await?;
        match self.cached_ranges(table) {
            Some(found) => Ok(found),
            None => bail!(
                "No shard ranges for table {table:?}; the shard_ranges table on the main \
                 database has no map for it. A table with no foreign key needs one seeded \
                 (see seed_shard_ranges); a table with a foreign key should be addressed \
                 through its parent instead."
            ),
        }
    }

    /// The instance holding one of `table`'s virtual shards, from that
    /// table's rows in the `shard_ranges` map.
    pub async fn partition_for_bucket(&self, table: &str, bucket: u32) -> Result<Partition> {
        if bucket >= VIRTUAL_SHARDS {
            bail!("Virtual shard {bucket} out of range");
        }
        let owner = |ranges: &Ranges| {
            let index = ranges.starts.partition_point(|&start| start <= bucket);
            index.checked_sub(1).map(|i| ranges.owners[i].clone())
        };
        if let Some(found) = owner(&self.table_ranges(table).await?) {
            return Ok(found);
        }
        // The map may have been extended since we last looked.
        self.refresh_shard_map().await?;
        match owner(&self.table_ranges(table).await?) {
            Some(found) => Ok(found),
            None => bail!(
                "No shard range of {table:?} covers virtual shard {bucket}; its rows in the \
                 shard_ranges table on the main database do not tile the space"
            ),
        }
    }

    /// The instance holding `table`'s row for `key` — hash the key, read
    /// that table's map. This is the only way a partition key is obtained
    /// for a table that hashes its own key; a table with a foreign key is
    /// reached through its parent's key instead.
    pub async fn partition_for_key(&self, table: &str, key: &str) -> Result<Partition> {
        self.partition_for_bucket(table, virtual_shard(key)).await
    }

    /// The instance holding everything keyed by this user id — the `users`
    /// row itself and every table whose first foreign key leads back to it.
    pub async fn partition_for_user(&self, user_id: &Uuid) -> Result<Partition> {
        self.partition_for_bucket(schema::USERS, virtual_shard_uuid(user_id))
            .await
    }

    /// The single instance holding *all* of `table` — for the tables whose
    /// map is one whole-space range, which is what makes a scan over non-key
    /// predicates answerable at all. Refuses a table that is spread, because
    /// for one there is no such instance and the caller has to name a key.
    pub async fn partition_for_table(&self, table: &str) -> Result<Partition> {
        if !schema::is_whole_space(table) {
            bail!(
                "{table:?} is not a whole-space table, so it has no single instance to name — \
                 look a row up by key with partition_for_key. (Refused on the declaration, not \
                 on how the map happens to be split today, so a table that is spread later \
                 doesn't silently start returning wrong answers.)"
            );
        }
        let ranges = self.table_ranges(table).await?;
        if ranges.owners.len() != 1 {
            bail!(
                "{table:?} is declared whole-space but its map has {} ranges; the shard_ranges \
                 rows for it are wrong",
                ranges.owners.len()
            );
        }
        Ok(ranges.owners[0].clone())
    }

    fn cached_pool(&self, key: &str) -> Option<PgPool> {
        self.map
            .read()
            .unwrap()
            .shards
            .get(key)
            .map(|shard| shard.pool.clone())
    }

    /// A connection pool for the database holding `partition`'s rows. `None`
    /// means the main/first database.
    pub async fn pool(&self, partition: &Partition) -> Result<PgPool> {
        let key = match partition.as_deref() {
            None | Some(MAIN_PARTITION_ROW_KEY) => return Ok(self.main.clone()),
            Some(key) => key,
        };
        if let Some(pool) = self.cached_pool(key) {
            return Ok(pool);
        }
        // Another instance may have added the partition since we last
        // looked; re-read the map once before giving up.
        self.refresh_shard_map().await?;
        match self.cached_pool(key) {
            Some(pool) => Ok(pool),
            None => bail!("Unknown partition key {key:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values from Python:
    // int.from_bytes(hashlib.blake2b(raw, digest_size=2).digest(), "big") % 65536
    #[test]
    fn virtual_shard_matches_python() {
        assert_eq!(virtual_shard("00000000-0000-0000-0000-000000000000"), 9348);
        assert_eq!(virtual_shard("3f2504e0-4f89-11d3-9a0c-0305e82c3301"), 21547);
        assert_eq!(virtual_shard("+14155552671"), 44330);
        assert_eq!(virtual_shard("api-1"), 44588);
        assert_eq!(virtual_shard("abc"), 44574);
    }

    #[test]
    fn even_ranges_tile_the_space() {
        let ranges = even_ranges(3);
        assert_eq!(ranges, vec![(0, 21845), (21846, 43690), (43691, 65535)]);
        assert_eq!(even_ranges(1), vec![(0, 65535)]);
    }

    fn settings(url: &str) -> Settings {
        // SAFETY: no other test reads or writes the environment.
        unsafe { std::env::set_var("USECODE_AGENT_NODE_NAME", "test") };
        let mut settings = Settings::from_env().unwrap();
        settings.database_url = url.to_string();
        settings
    }

    #[test]
    fn shard_urls_reuse_driver_and_credentials() {
        let settings = settings("postgresql+asyncpg://u:p@Postgres-1:5432/main?x=1");
        assert_eq!(
            main_target(&settings),
            Target {
                host: "postgres-1".into(),
                port: 5432,
                database: "main".into()
            }
        );
        assert_eq!(
            shard_url(&settings, &split_target("postgres-2:5433/other")),
            "postgresql+asyncpg://u:p@postgres-2:5433/other?x=1"
        );
        assert_eq!(
            driver_url("postgresql+asyncpg://u:p@h:1/d"),
            "postgresql://u:p@h:1/d"
        );
        assert_eq!(split_target("h/d").port, 5432);
    }
}
