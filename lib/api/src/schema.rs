// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Where every table's rows live. Every table is partitioned, directly or
//! through its parent. There are no exceptions and no "global" tables.
//!
//! A table with **no foreign key** is partitioned on its **own key**: hash
//! it into a virtual shard (`db::virtual_shard`), look the bucket up in the
//! `shard_ranges` rows for that table, and that is the instance holding the
//! row. `users` hashes the user id; `user_directory` and `otps` hash the
//! phone number, which is why they need maps of their own rather than
//! following the user's; `api_keys` hashes the key hash; `tasks` hashes the
//! assignee. It does *not* mean the row is on the main database — the main
//! database is simply where a bucket lands when the map points there.
//!
//! `api_keys` is a root table for the same reason `otps` is one: a request
//! carrying an API key has no user id yet — the key is what produces one —
//! so the key hash is the only address it can have. Its `user_id` is a
//! plain column rather than a foreign key, since the owning `users` row
//! hashes elsewhere; `user_api_keys` is the index that answers the reverse
//! question.
//!
//! `tasks` is a root table for a different reason: not because of what a
//! request carries, but because of how the rows are *read*. A task is never
//! looked up by a user asking for it — each API instance sweeps its own
//! outstanding work on a timer — so the query that has to be cheap is "this
//! node's tasks", and hashing the assignee answers it on one instance
//! instead of a walk across every one of them. Its `user_id` is a plain
//! column too, and `user_tasks` is its reverse index.
//!
//! A table **with** a foreign key lives on the instance holding the row its
//! *first* foreign key points at — no hashing of its own, it inherits its
//! parent's placement, so it needs no `shard_ranges` rows at all. Later
//! foreign keys are ordinary references and say nothing about placement. A
//! column that names a row on another instance — `web_sessions.api_key_hash`,
//! `api_keys.user_id` — is deliberately *not* a foreign key, because no
//! database could enforce it.
//!
//! Every lookup therefore names a table: `Db::partition_for_key(table, key)`
//! reads that table's map, and `Db::partition_for_table(table)` answers for
//! a table whose whole bucket space is one range. The catalog tables
//! (`server_type_mappings`, `location_mappings`, `provider_resources`) are
//! of that second kind, because they are read by scans over non-key
//! predicates rather than by their key — a hash could not address those
//! queries. So is `shard_ranges` itself, by necessity: it is the map, so it
//! has to be readable before any map has been read.
//!
//! None of this is left to be inferred: every table states its placement
//! in [`TABLES`], and [`check_placements`] refuses to start if a
//! declaration disagrees with the schema the migrations actually built.

use anyhow::{Result, bail};
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Partitioned on this table's own key: `db::virtual_shard(key)` picks
    /// the bucket, and this table's rows in `shard_ranges` say which
    /// instance owns it. For tables with no foreign key, which therefore
    /// have nothing to inherit placement from. The key must be a
    /// primary-key column, since a row has to be addressable by what
    /// callers actually hold.
    HashedOn(&'static str),
    /// Lives on the instance holding the row `column` points at — the
    /// table's *first* foreign key. No hashing and no `shard_ranges` rows of
    /// its own; being co-located with the parent is also what lets `column`
    /// be a real foreign key.
    InheritsFrom {
        column: &'static str,
        parent: &'static str,
    },
    /// One range covering every bucket, on the main database. Not an
    /// exception to the scheme — a map like any other, whose answer happens
    /// not to vary by bucket. The reason is required because "don't hash
    /// this one" always needs one.
    WholeSpace(&'static str),
}

pub struct Table {
    pub name: &'static str,
    pub placement: Placement,
}

pub const SHARD_RANGES: &str = "shard_ranges";
pub const USER_DIRECTORY: &str = "user_directory";
pub const USERS: &str = "users";
pub const OTPS: &str = "otps";
pub const API_KEYS: &str = "api_keys";
pub const TASKS: &str = "tasks";
pub const SERVER_TYPE_MAPPINGS: &str = "server_type_mappings";
pub const LOCATION_MAPPINGS: &str = "location_mappings";
pub const PROVIDER_RESOURCES: &str = "provider_resources";

const OWNED_BY_USER: Placement = Placement::InheritsFrom {
    column: "user_id",
    parent: USERS,
};

pub const TABLES: &[Table] = &[
    Table {
        // Which physical instance holds a contiguous run of virtual shards,
        // for one table. Only the main database's copy is ever read.
        name: SHARD_RANGES,
        placement: Placement::WholeSpace(
            "it is the map, so it has to be readable before any map has been read",
        ),
    },
    Table {
        // Phone number -> user id: at login there is no user id yet, so this
        // is what turns the phone into one. Its rows do not follow the
        // user's, which is exactly why it needs a map of its own.
        name: USER_DIRECTORY,
        placement: Placement::HashedOn("phone"),
    },
    Table {
        // The root of the user-owned tree.
        name: USERS,
        placement: Placement::HashedOn("id"),
    },
    Table {
        // A pending login code; it exists before the user it will log in
        // does, so it cannot be keyed by user id.
        name: OTPS,
        placement: Placement::HashedOn("phone"),
    },
    Table {
        // A bearer token, found by the token and nothing else.
        name: API_KEYS,
        placement: Placement::HashedOn("key_hash"),
    },
    Table {
        // A user's own index of the API keys they were issued.
        name: "user_api_keys",
        placement: OWNED_BY_USER,
    },
    Table {
        // A browser cookie, only ever presented after the user is known.
        name: "web_sessions",
        placement: OWNED_BY_USER,
    },
    Table {
        name: "provider_credentials",
        placement: OWNED_BY_USER,
    },
    Table {
        name: SERVER_TYPE_MAPPINGS,
        placement: Placement::WholeSpace(
            "read by scans over non-key predicates — every series a provider offers — \
             which hashing the series would not address",
        ),
    },
    Table {
        name: LOCATION_MAPPINGS,
        placement: Placement::WholeSpace(
            "read by scans over non-key predicates — every city known for a provider — \
             which hashing the code would not address",
        ),
    },
    Table {
        name: PROVIDER_RESOURCES,
        placement: Placement::WholeSpace(
            "read by scans over non-key predicates — every resource of a kind a provider \
             has — which hashing the code would not address",
        ),
    },
    Table {
        name: "servers",
        placement: OWNED_BY_USER,
    },
    Table {
        // Partitioned on the node carrying the task: that is how tasks are
        // read — every instance sweeps its own outstanding work.
        name: TASKS,
        placement: Placement::HashedOn("assignee"),
    },
    Table {
        // A user's own index of the tasks running on their behalf.
        name: "user_tasks",
        placement: OWNED_BY_USER,
    },
];

pub fn placement_of(table: &str) -> Option<Placement> {
    TABLES.iter().find(|t| t.name == table).map(|t| t.placement)
}

pub fn is_whole_space(table: &str) -> bool {
    matches!(placement_of(table), Some(Placement::WholeSpace(_)))
}

/// Every table that needs its own map: the ones that hash their own key or
/// cover the whole space, as opposed to the ones that inherit a parent's
/// placement.
pub fn mapped_tables() -> Vec<&'static str> {
    let mut tables: Vec<_> = TABLES
        .iter()
        .filter(|t| !matches!(t.placement, Placement::InheritsFrom { .. }))
        .map(|t| t.name)
        .collect();
    tables.sort_unstable();
    tables
}

/// Fail if any declared placement disagrees with the schema on `pool`.
/// Called at startup, before the shard map is seeded: a placement that has
/// drifted from the foreign keys would route queries to the wrong instance,
/// which no later check would notice.
pub async fn check_placements(pool: &PgPool) -> Result<()> {
    for table in TABLES {
        let name = table.name;
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT column_name::text FROM information_schema.columns \
             WHERE table_schema = current_schema() AND table_name = $1",
        )
        .bind(name)
        .fetch_all(pool)
        .await?;
        if columns.is_empty() {
            bail!("{name} is declared in schema::TABLES but does not exist in the database");
        }

        // (column, parent table) of every foreign-key column, in column order.
        let foreign_keys: Vec<(String, String)> = sqlx::query_as(
            "SELECT a.attname::text, parent.relname::text \
             FROM pg_constraint c \
             JOIN pg_class t ON t.oid = c.conrelid \
             JOIN pg_namespace n ON n.oid = t.relnamespace \
             JOIN pg_class parent ON parent.oid = c.confrelid \
             JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(c.conkey) \
             WHERE c.contype = 'f' AND n.nspname = current_schema() AND t.relname = $1 \
             ORDER BY a.attnum",
        )
        .bind(name)
        .fetch_all(pool)
        .await?;
        let first_fk = foreign_keys.first().map(|(column, _)| column.as_str());

        match table.placement {
            Placement::InheritsFrom { column, parent } => {
                let Some(first) = first_fk else {
                    bail!("{name} inherits placement from {column:?} but has no foreign key");
                };
                if first != column {
                    bail!(
                        "{name} inherits placement from {column:?}, but its first foreign key \
                         is {first:?} — placement follows the first one"
                    );
                }
                let mut parents: Vec<&str> = foreign_keys
                    .iter()
                    .filter(|(c, _)| c == first)
                    .map(|(_, p)| p.as_str())
                    .collect();
                parents.dedup();
                if parents != [parent] {
                    bail!(
                        "{name} declares parent {parent:?} but {name}.{first} points at {parents:?}"
                    );
                }
            }
            Placement::HashedOn(key) | Placement::WholeSpace(key) => {
                if let Some(first) = first_fk {
                    bail!(
                        "{name} is declared {:?}, which needs its own shard_ranges map, but it \
                         has a foreign key on {first:?} and so inherits its parent's instance — \
                         declare InheritsFrom instead",
                        table.placement
                    );
                }
                if matches!(table.placement, Placement::WholeSpace(_)) {
                    continue;
                }
                if !columns.iter().any(|c| c == key) {
                    bail!("{name} hashes on {key:?}, which is not a column of {name}");
                }
                let primary_key: Vec<String> = sqlx::query_scalar(
                    "SELECT a.attname::text FROM pg_index i \
                     JOIN pg_class t ON t.oid = i.indrelid \
                     JOIN pg_namespace n ON n.oid = t.relnamespace \
                     JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(i.indkey) \
                     WHERE i.indisprimary AND n.nspname = current_schema() AND t.relname = $1",
                )
                .bind(name)
                .fetch_all(pool)
                .await?;
                if !primary_key.iter().any(|c| c == key) {
                    bail!(
                        "{name} hashes on {key:?}, which is not part of its primary key — a \
                         row has to be addressable by the key callers hold"
                    );
                }
            }
        }
    }
    Ok(())
}
