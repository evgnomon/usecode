// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Schema migrations: every `migrations/NNNN_name.sql`, built into the
//! binary by `build.rs` and applied in order to every database instance.
//!
//! All instances carry the identical schema - what differs is which rows
//! are on which - so the same set is applied to each, taken from static
//! configuration since the shard map can't be read before it exists.
//!
//! They run on every start, and on their own with `uc-agent-api migrate`
//! (`--status` only says where each database is). What has been applied is
//! the `migrations` ledger: one row per migration with the checksum of the
//! file it ran, when, and which instance ran it.
//!
//! Several API instances start at once, and during a rolling deploy old and
//! new builds run side by side, so:
//!
//! - Each database is upgraded under a PostgreSQL advisory lock. One
//!   instance applies what is pending; the others wait for it, then find
//!   nothing left to do.
//! - Each migration is a transaction of its own, ledger row included, with
//!   a `lock_timeout` so a busy table can't stall the site behind it. A
//!   failure leaves the database at the last migration that went in, and
//!   the next start carries on from there.
//! - A build that finds migrations newer than any it knows still starts:
//!   that is the old replica during a rollout. So a migration only ever
//!   adds what the running build can live without (expand), and what goes
//!   away goes in a later release (contract).
//! - It refuses to start when an applied file was edited (its checksum
//!   changed), when the database has a migration the build doesn't know in
//!   among ones it does, or when one is missing before ones that are
//!   applied: each means the history was rewritten.
//!
//! A database the earlier Alembic setup made is adopted on first sight:
//! every migration up to its `alembic_version` is entered in the ledger,
//! and `alembic_version` is dropped.

use std::env;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgConnectOptions, PgConnection};
use sqlx::{Connection, Postgres, Transaction};

use crate::config::Settings;
use crate::db;

// Advisory lock id held while a database is migrated. Any fixed 64-bit
// integer works - it just has to be the same one in every instance.
const MIGRATION_LOCK_ID: i64 = 0x7245_5341_4E41_4B00;

/// How long a migration may wait for a lock on a table in use before it
/// gives up, rather than holding up every query queued behind it.
const LOCK_TIMEOUT: &str = "SET LOCAL lock_timeout = '60s'";

/// How long to keep trying to reach a database that is still starting.
const CONNECT_ATTEMPTS: u32 = 30;
const CONNECT_PAUSE: Duration = Duration::from_secs(2);

const LEDGER: &str = "CREATE TABLE IF NOT EXISTS migrations (
    version    text PRIMARY KEY,
    checksum   text NOT NULL,
    applied_at timestamptz NOT NULL DEFAULT now(),
    applied_by text NOT NULL
)";

#[derive(Debug)]
pub struct Migration {
    /// The file's name without `.sql`, e.g. `0001_init`.
    pub version: &'static str,
    pub sql: &'static str,
}

impl Migration {
    fn checksum(&self) -> String {
        Sha256::digest(self.sql.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

/// Every migration this build has, in order.
pub const MIGRATIONS: &[Migration] = include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

/// A row of the ledger.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct Applied {
    version: String,
    checksum: String,
}

/// What a database needs from this build.
#[derive(Debug)]
struct Plan<'a> {
    pending: Vec<&'a Migration>,
    /// Applied migrations newer than this build's last one.
    newer: Vec<String>,
}

/// Compare what is applied with what this build has.
fn plan<'a>(known: &'a [Migration], applied: &[Applied]) -> Result<Plan<'a>> {
    let head = known.last().map(|m| m.version).unwrap_or_default();
    let mut newer = Vec::new();
    for a in applied {
        match known.iter().find(|m| m.version == a.version) {
            Some(m) if m.checksum() != a.checksum => bail!(
                "migration {} was changed after it was applied; put the change back and add \
                 it as a new migration instead",
                a.version
            ),
            Some(_) => {}
            None if a.version.as_str() > head => newer.push(a.version.clone()),
            None => bail!(
                "the database has migration {} applied, which this build doesn't have; was it \
                 renamed or removed?",
                a.version
            ),
        }
    }
    let pending: Vec<_> = known
        .iter()
        .filter(|m| !applied.iter().any(|a| a.version == m.version))
        .collect();
    let last_applied = applied.iter().map(|a| a.version.as_str()).max();
    if let (Some(first), Some(last)) = (pending.first(), last_applied)
        && first.version < last
    {
        bail!(
            "migration {} is not applied, but the later {last} is; renumber it to come after \
             {last}",
            first.version
        );
    }
    newer.sort();
    Ok(Plan { pending, newer })
}

/// Bring every configured database instance up to date.
pub async fn run(settings: &Settings) -> Result<()> {
    let applied_by = applied_by(settings);
    for (partition, url) in db::configured_urls(settings) {
        let label = partition.as_deref().unwrap_or("main");
        let mut conn = connect(label, &url).await?;
        let applied = upgrade(&mut conn, MIGRATIONS, &applied_by)
            .await
            .with_context(|| format!("migrating the {label} database"))?;
        if applied.is_empty() {
            tracing::info!("The {label} database is up to date");
        }
        conn.close().await?;
    }
    Ok(())
}

/// Say where every configured database is, without changing any.
pub async fn status(settings: &Settings) -> Result<()> {
    for (partition, url) in db::configured_urls(settings) {
        let label = partition.as_deref().unwrap_or("main");
        let mut conn = connect(label, &url).await?;
        let applied = applied(&mut conn, MIGRATIONS).await?;
        let plan = plan(MIGRATIONS, &applied).with_context(|| format!("the {label} database"))?;
        let at = applied
            .last()
            .map(|a| a.version.as_str())
            .unwrap_or("nothing");
        println!("{label}: {} applied, at {at}", applied.len());
        for m in &plan.pending {
            println!("  pending {}", m.version);
        }
        for v in &plan.newer {
            println!("  newer than this build: {v}");
        }
        conn.close().await?;
    }
    Ok(())
}

async fn connect(label: &str, url: &str) -> Result<PgConnection> {
    let options: PgConnectOptions = db::driver_url(url)
        .parse()
        .with_context(|| format!("the {label} database URL"))?;
    let mut attempt = 1;
    loop {
        match PgConnection::connect_with(&options).await {
            Ok(conn) => return Ok(conn),
            Err(err) if attempt < CONNECT_ATTEMPTS => {
                tracing::info!("Waiting for the {label} database: {err}");
                tokio::time::sleep(CONNECT_PAUSE).await;
                attempt += 1;
            }
            Err(err) => {
                return Err(err).with_context(|| format!("connecting to the {label} database"));
            }
        }
    }
}

/// Who the ledger says applied a migration: the instance's name, else the
/// host's.
fn applied_by(settings: &Settings) -> String {
    [Some(settings.node_name.clone()), env::var("HOSTNAME").ok()]
        .into_iter()
        .flatten()
        .map(|name| name.trim().to_string())
        .find(|name| !name.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Apply what is pending to one database, waiting for any other instance
/// doing the same. Returns the versions applied.
async fn upgrade(
    conn: &mut PgConnection,
    known: &[Migration],
    applied_by: &str,
) -> Result<Vec<String>> {
    // A session lock, not a transaction's: it has to span one transaction
    // per migration.
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(MIGRATION_LOCK_ID)
        .fetch_one(&mut *conn)
        .await?;
    if !locked {
        tracing::info!("Another instance is migrating this database; waiting for it");
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(MIGRATION_LOCK_ID)
            .execute(&mut *conn)
            .await?;
    }
    let result = upgrade_locked(conn, known, applied_by).await;
    // Also released when the connection closes, should this fail.
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(MIGRATION_LOCK_ID)
        .execute(&mut *conn)
        .await;
    result
}

async fn upgrade_locked(
    conn: &mut PgConnection,
    known: &[Migration],
    applied_by: &str,
) -> Result<Vec<String>> {
    sqlx::raw_sql(LEDGER).execute(&mut *conn).await?;
    adopt_alembic(conn, known).await?;
    let applied = read_ledger(conn).await?;
    let plan = plan(known, &applied)?;
    if !plan.newer.is_empty() {
        tracing::warn!(
            "The database has migrations newer than this build ({}); carrying on, as they \
             only add to the schema",
            plan.newer.join(", ")
        );
    }

    let mut done = Vec::new();
    for migration in plan.pending {
        tracing::info!("Applying migration {}", migration.version);
        let mut tx = conn.begin().await?;
        sqlx::raw_sql(LOCK_TIMEOUT).execute(&mut *tx).await?;
        before(migration.version, &mut tx).await?;
        sqlx::raw_sql(migration.sql)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("applying migration {}", migration.version))?;
        sqlx::query("INSERT INTO migrations (version, checksum, applied_by) VALUES ($1, $2, $3)")
            .bind(migration.version)
            .bind(migration.checksum())
            .bind(applied_by)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        done.push(migration.version.to_string());
    }
    Ok(done)
}

async fn read_ledger(conn: &mut PgConnection) -> Result<Vec<Applied>> {
    Ok(
        sqlx::query_as("SELECT version, checksum FROM migrations ORDER BY version")
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// What a database has applied, read without changing it: the ledger, or
/// what an Alembic-made database's revision amounts to.
async fn applied(conn: &mut PgConnection, known: &[Migration]) -> Result<Vec<Applied>> {
    if has_table(conn, "migrations").await? {
        return read_ledger(conn).await;
    }
    alembic_applied(conn, known).await
}

async fn has_table(conn: &mut PgConnection, table: &str) -> Result<bool> {
    Ok(sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(table)
        .fetch_one(&mut *conn)
        .await?)
}

/// The migrations up to an Alembic-made database's revision, as ledger
/// rows; none if there is no `alembic_version`.
async fn alembic_applied(conn: &mut PgConnection, known: &[Migration]) -> Result<Vec<Applied>> {
    if !has_table(conn, "alembic_version").await? {
        return Ok(Vec::new());
    }
    let revision: Option<String> = sqlx::query_scalar("SELECT version_num FROM alembic_version")
        .fetch_optional(&mut *conn)
        .await?;
    let Some(revision) = revision else {
        return Ok(Vec::new());
    };
    let Some(index) = known.iter().position(|m| m.version == revision) else {
        bail!("the database is at Alembic revision {revision:?}, which this build doesn't have");
    };
    Ok(known[..=index]
        .iter()
        .map(|m| Applied {
            version: m.version.to_string(),
            checksum: m.checksum(),
        })
        .collect())
}

/// Enter an Alembic-made database's history in the ledger and drop
/// `alembic_version`, in one go.
async fn adopt_alembic(conn: &mut PgConnection, known: &[Migration]) -> Result<()> {
    if !has_table(conn, "alembic_version").await? {
        return Ok(());
    }
    let adopted = alembic_applied(conn, known).await?;
    let mut tx = conn.begin().await?;
    for a in &adopted {
        sqlx::query(
            "INSERT INTO migrations (version, checksum, applied_by) VALUES ($1, $2, 'alembic') \
             ON CONFLICT (version) DO NOTHING",
        )
        .bind(&a.version)
        .bind(&a.checksum)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::raw_sql("DROP TABLE alembic_version")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    if let Some(last) = adopted.last() {
        tracing::info!("Adopted the Alembic history, up to {}", last.version);
    }
    Ok(())
}

/// Data steps a migration needs from the environment, run just before its
/// SQL.
async fn before(revision: &str, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    if revision == "0012_tasks_partition_on_assignee" {
        // Tasks with a null assignee belonged to the main API instance by the
        // old convention. `USECODE_AGENT_MAIN_NODE_NAME` names it explicitly;
        // failing that, a single-instance deployment's own name is the same
        // answer. Whatever is left unassigned is deleted by the migration.
        let main_node = ["USECODE_AGENT_MAIN_NODE_NAME", "USECODE_AGENT_NODE_NAME"]
            .iter()
            .filter_map(|name| env::var(name).ok())
            .map(|name| name.trim().to_string())
            .find(|name| !name.is_empty());
        if let Some(node) = main_node {
            sqlx::query("UPDATE tasks SET assignee = $1 WHERE assignee IS NULL")
                .bind(node)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Migration = Migration {
        version: "0001_a",
        sql: "CREATE TABLE a (id int)",
    };
    const B: Migration = Migration {
        version: "0002_b",
        sql: "CREATE TABLE b (id int)",
    };
    const C: Migration = Migration {
        version: "0003_c",
        sql: "CREATE TABLE c (id int)",
    };

    fn ok(m: &Migration) -> Applied {
        Applied {
            version: m.version.into(),
            checksum: m.checksum(),
        }
    }

    fn versions(plan: &Plan<'_>) -> Vec<&'static str> {
        plan.pending.iter().map(|m| m.version).collect()
    }

    #[test]
    fn the_built_in_migrations_are_in_order() {
        assert!(MIGRATIONS.len() >= 12);
        assert_eq!(MIGRATIONS[0].version, "0001_init");
        assert!(MIGRATIONS.windows(2).all(|w| w[0].version < w[1].version));
    }

    #[test]
    fn an_empty_database_gets_everything() {
        let plan = plan(&[A, B], &[]).unwrap();
        assert_eq!(versions(&plan), ["0001_a", "0002_b"]);
    }

    #[test]
    fn only_what_is_missing_is_pending() {
        let plan = plan(&[A, B, C], &[ok(&A)]).unwrap();
        assert_eq!(versions(&plan), ["0002_b", "0003_c"]);
        assert!(plan.newer.is_empty());
    }

    #[test]
    fn an_older_build_starts_on_a_newer_schema() {
        let plan = plan(&[A], &[ok(&A), ok(&B)]).unwrap();
        assert!(plan.pending.is_empty());
        assert_eq!(plan.newer, ["0002_b"]);
    }

    #[test]
    fn an_edited_migration_is_refused() {
        let mut edited = ok(&A);
        edited.checksum = "0".repeat(64);
        let err = plan(&[A, B], &[edited]).unwrap_err().to_string();
        assert!(err.contains("changed after it was applied"), "{err}");
    }

    #[test]
    fn an_unknown_migration_among_known_ones_is_refused() {
        let gone = Applied {
            version: "0001_gone".into(),
            checksum: String::new(),
        };
        let err = plan(&[A, B], &[gone]).unwrap_err().to_string();
        assert!(err.contains("which this build doesn't have"), "{err}");
    }

    #[test]
    fn a_migration_missing_before_applied_ones_is_refused() {
        let err = plan(&[A, B, C], &[ok(&A), ok(&C)]).unwrap_err().to_string();
        assert!(err.contains("0002_b is not applied"), "{err}");
    }

    /// A database of its own for one test, on the Postgres at
    /// `USECODE_AGENT_TEST_DATABASE_URL` (`make test-db`); `None` skips the
    /// test when there isn't one.
    async fn fresh_database() -> Option<PgConnectOptions> {
        let url = env::var("USECODE_AGENT_TEST_DATABASE_URL").ok()?;
        let options: PgConnectOptions = url.parse().unwrap();
        let mut admin = match PgConnection::connect_with(&options).await {
            Ok(conn) => conn,
            Err(err) => {
                eprintln!("skipped, no test database: {err}");
                return None;
            }
        };
        let name = format!("migrate_{}", uuid::Uuid::new_v4().simple());
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&mut admin)
            .await
            .unwrap();
        Some(options.database(&name))
    }

    async fn ledger(options: &PgConnectOptions) -> Vec<(String, String)> {
        let mut conn = PgConnection::connect_with(options).await.unwrap();
        sqlx::query_as("SELECT version, applied_by FROM migrations ORDER BY version")
            .fetch_all(&mut conn)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn racing_instances_apply_each_migration_once() {
        let Some(options) = fresh_database().await else {
            return;
        };
        let runs = (0..6).map(|i| {
            let options = options.clone();
            tokio::spawn(async move {
                let mut conn = PgConnection::connect_with(&options).await.unwrap();
                upgrade(&mut conn, MIGRATIONS, &format!("api-{i}"))
                    .await
                    .unwrap()
            })
        });
        let mut applied: Vec<String> = Vec::new();
        for run in runs.collect::<Vec<_>>() {
            applied.extend(run.await.unwrap());
        }
        applied.sort();
        let all: Vec<String> = MIGRATIONS.iter().map(|m| m.version.to_string()).collect();
        assert_eq!(
            applied, all,
            "each migration is applied by exactly one instance"
        );
        assert_eq!(ledger(&options).await.len(), MIGRATIONS.len());

        let mut conn = PgConnection::connect_with(&options).await.unwrap();
        assert!(
            upgrade(&mut conn, MIGRATIONS, "api-9")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn an_alembic_database_is_adopted() {
        let Some(options) = fresh_database().await else {
            return;
        };
        let mut conn = PgConnection::connect_with(&options).await.unwrap();
        let (old, new) = MIGRATIONS.split_at(5);
        for m in old {
            sqlx::raw_sql(m.sql).execute(&mut conn).await.unwrap();
        }
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE TABLE alembic_version (version_num varchar(32) PRIMARY KEY);
             INSERT INTO alembic_version VALUES ('{}')",
            old.last().unwrap().version
        )))
        .execute(&mut conn)
        .await
        .unwrap();

        let applied = upgrade(&mut conn, MIGRATIONS, "api-1").await.unwrap();
        let expected: Vec<String> = new.iter().map(|m| m.version.to_string()).collect();
        assert_eq!(applied, expected);
        assert!(!has_table(&mut conn, "alembic_version").await.unwrap());
        let ledger = ledger(&options).await;
        assert_eq!(ledger.len(), MIGRATIONS.len());
        assert_eq!(ledger[0].1, "alembic");
    }

    #[tokio::test]
    async fn an_older_build_leaves_a_newer_schema_alone() {
        let Some(options) = fresh_database().await else {
            return;
        };
        let mut conn = PgConnection::connect_with(&options).await.unwrap();
        upgrade(&mut conn, &[A, B], "new").await.unwrap();
        assert!(upgrade(&mut conn, &[A], "old").await.unwrap().is_empty());
        let err = upgrade(&mut conn, &[A, C], "odd").await.unwrap_err();
        assert!(
            err.to_string().contains("which this build doesn't have"),
            "{err}"
        );
    }
}
