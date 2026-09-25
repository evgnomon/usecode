// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Schema migrations, applied to every database instance at startup.
//!
//! All instances carry the identical schema — what differs is which rows are
//! on which — so the same migration set is applied to each, taken from
//! static configuration since the shard-mapping table can't be read before
//! it exists.
//!
//! The revision ids and the `alembic_version` bookkeeping table are the ones
//! the Python implementation's Alembic migrations used, so a database it
//! created is picked up at whatever revision it had reached.
//!
//! Several API instances boot at once behind the load balancer and would
//! otherwise race here, so each upgrade runs in one transaction under a
//! PostgreSQL advisory lock; the instances that lose the race simply find
//! the database already at head.

use std::env;

use anyhow::{Context, Result, bail};
use sqlx::postgres::PgConnection;
use sqlx::{Connection, Postgres, Transaction};

use crate::config::Settings;
use crate::db;

// Advisory lock id held for the duration of a migration. Any fixed 64-bit
// integer works — it just has to be the same one in every instance.
const MIGRATION_LOCK_ID: i64 = 0x7245_5341_4E41_4B00;

struct Migration {
    revision: &'static str,
    sql: &'static str,
}

macro_rules! migration {
    ($revision:literal, $file:literal) => {
        Migration {
            revision: $revision,
            sql: include_str!(concat!("../migrations/", $file)),
        }
    };
}

const MIGRATIONS: &[Migration] = &[
    migration!("0001_init", "0001_init.sql"),
    migration!("0002_provider_credentials", "0002_provider_credentials.sql"),
    migration!("0003_servers", "0003_servers.sql"),
    migration!("0004_tasks", "0004_tasks.sql"),
    migration!("0005_provider_catalog", "0005_provider_catalog.sql"),
    migration!("0006_server_type_cities", "0006_server_type_cities.sql"),
    migration!("0007_location_mapping", "0007_location_mapping.sql"),
    migration!("0008_horizontal_scaling", "0008_horizontal_scaling.sql"),
    migration!("0009_virtual_shards", "0009_virtual_shards.sql"),
    migration!(
        "0010_shard_ranges_per_table",
        "0010_shard_ranges_per_table.sql"
    ),
    migration!("0011_api_keys_own_shard", "0011_api_keys_own_shard.sql"),
    migration!(
        "0012_tasks_partition_on_assignee",
        "0012_tasks_partition_on_assignee.sql"
    ),
];

pub fn head() -> &'static str {
    MIGRATIONS.last().map(|m| m.revision).unwrap_or_default()
}

/// Bring every configured database instance to head.
pub async fn run(settings: &Settings) -> Result<()> {
    for (partition, url) in db::configured_urls(settings) {
        tracing::info!(
            "Migrating database for partition {}",
            partition.as_deref().unwrap_or("<main>")
        );
        let mut conn = PgConnection::connect(&db::driver_url(&url))
            .await
            .with_context(|| {
                format!(
                    "connecting to the {} database",
                    partition.as_deref().unwrap_or("main")
                )
            })?;
        upgrade(&mut conn).await?;
        conn.close().await?;
    }
    Ok(())
}

async fn upgrade(conn: &mut PgConnection) -> Result<()> {
    let mut tx = conn.begin().await?;
    // Serialize concurrent upgrades of this database; released with the
    // transaction, whether it commits or rolls back.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(MIGRATION_LOCK_ID)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS alembic_version (
             version_num VARCHAR(32) NOT NULL,
             CONSTRAINT alembic_version_pkc PRIMARY KEY (version_num)
         )",
    )
    .execute(&mut *tx)
    .await?;

    let current: Option<String> = sqlx::query_scalar("SELECT version_num FROM alembic_version")
        .fetch_optional(&mut *tx)
        .await?;
    let pending = match &current {
        None => MIGRATIONS,
        Some(current) => match MIGRATIONS.iter().position(|m| m.revision == current) {
            Some(index) => &MIGRATIONS[index + 1..],
            None => bail!(
                "database is at unknown revision {current:?}; this build knows up to {:?}",
                head()
            ),
        },
    };

    let mut previous = current.as_deref().unwrap_or("<base>");
    for migration in pending {
        tracing::info!("Running upgrade {previous} -> {}", migration.revision);
        previous = migration.revision;
        before(migration.revision, &mut tx).await?;
        sqlx::raw_sql(migration.sql)
            .execute(&mut *tx)
            .await
            .with_context(|| format!("applying migration {}", migration.revision))?;
        sqlx::query("DELETE FROM alembic_version")
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO alembic_version (version_num) VALUES ($1)")
            .bind(migration.revision)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
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
