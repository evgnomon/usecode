// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Task engine: resumable workflows over provider-owned resources.
//!
//! A `tasks` row is a suspended async function. `state` is the step it is
//! parked at; a step handler runs, does one unit of work (usually one
//! provider API call or one poll), and returns either the next state to
//! suspend at or [`Next::Done`] once the workflow has finished. A step that
//! finishes a task is responsible for applying the effect to our own
//! resource tables (e.g. deleting the `servers` row) *before* returning
//! `Done` — the engine then deletes the task row itself, so a task never
//! outlives the resource mutation it exists to perform.
//!
//! Step handlers are looked up per (kind, state) — see
//! `server_tasks::Step::for_task`. A given `kind` (e.g. "delete_server") is a
//! fixed sequence of named states, similar to labelled steps in an async
//! function that can await external events between them.
//!
//! Because the API runs as several named instances behind a load balancer,
//! every task has an **assignee**: the node name of the instance that picked
//! it up. Only the assignee sweeps it, so two instances never run the same
//! step concurrently — and because that is how tasks are read, it is also
//! where they are stored: `tasks` is partitioned on the assignee, so an
//! instance's whole sweep is one query on one database. A task is addressed
//! as (assignee, id), and the assignee travels with the task id everywhere
//! in this module for that reason.
//!
//! The rows a step *mutates* are a different matter: those belong to the
//! user, on the instance their owner hashes to. Every context therefore
//! carries both — `assignee` for the task itself, `user_partition` for
//! everything the handler touches on the user's behalf.

use anyhow::{Result, anyhow};
use serde_json::Value;
use uuid::Uuid;

use crate::App;
use crate::db::Partition;
use crate::server_tasks::Step;
use crate::store::TaskRecord;

pub struct TaskContext {
    pub user_id: Uuid,
    // The database instance the *user's* rows live on — servers, provider
    // credentials, everything a step actually mutates. Not where the task
    // itself is: step handlers pass this to every partitioned store call
    // they make.
    pub user_partition: Partition,
    pub resources: Value,
    pub payload: Value,
}

/// What a step handler returns.
pub enum Next {
    /// The workflow is finished; its effect has been applied.
    Done,
    /// Suspend at `state`; `payload` replaces the task's payload.
    Park { state: &'static str, payload: Value },
}

/// Create a task assigned to *this* API instance — the one handling the
/// request is the one that picked the work up, and it is the one that will
/// carry it to completion. That assignee is also the task's address, so the
/// row lands on the instance this node's name hashes to, wherever the user's
/// own rows may be.
pub async fn create_task(
    app: &App,
    user_id: &Uuid,
    kind: &str,
    initial_state: &str,
    resources: Value,
    payload: Value,
) -> Result<TaskRecord> {
    app.store
        .create_task(
            user_id,
            kind,
            &app.settings.node_name,
            initial_state,
            &resources,
            &payload,
        )
        .await
}

/// Run the task's current step once. Returns the task's new state, or
/// `None` if the task is no longer around (finished, or never existed).
///
/// Only the assignee calls this — it names the node whose database holds the
/// task, so a caller that isn't that node has nothing to advance.
pub async fn advance(app: &App, assignee: &str, task_id: &Uuid) -> Result<Option<TaskRecord>> {
    let Some(task) = app.store.get_task(assignee, task_id).await? else {
        return Ok(None);
    };
    let step = Step::for_task(&task.kind, &task.state).ok_or_else(|| {
        anyhow!(
            "No step handler for task kind={:?} state={:?}",
            task.kind,
            task.state
        )
    })?;

    let ctx = TaskContext {
        user_id: task.user_id,
        // Resolved from the owner, not from where the task itself sits: the
        // two are unrelated instances since tasks hash on assignee.
        user_partition: app.store.db().partition_for_user(&task.user_id).await?,
        resources: task.resources.clone(),
        payload: task.payload.clone(),
    };

    match step.run(app, &ctx).await {
        Err(err) => {
            // A failing step is otherwise invisible: the error is persisted
            // on the row and nothing reaches the log, so a task retrying
            // forever looks like silence to an operator.
            tracing::warn!(
                "Task {} (kind={}) failed at state {}: {err:#}",
                task.id,
                task.kind,
                task.state
            );
            // Compare-and-set on the state we read: if a concurrent run of
            // this same step already advanced the task, recording our
            // failure here would drag it back a step and wedge the workflow.
            // The task stays at its current state and is retried.
            app.store
                .set_task_state(
                    assignee,
                    &task.id,
                    &task.state,
                    None,
                    Some(&format!("{err:#}")),
                    Some(&task.state),
                )
                .await?;
            app.store.get_task(assignee, &task.id).await
        }
        Ok(Next::Done) => {
            // The handler already applied its effect to the resource
            // table(s); the workflow is complete, so the task goes away.
            app.store.delete_task(assignee, &task.id).await?;
            Ok(None)
        }
        Ok(Next::Park { state, payload }) => {
            app.store
                .set_task_state(
                    assignee,
                    &task.id,
                    state,
                    Some(&payload),
                    None,
                    Some(&task.state),
                )
                .await?;
            app.store.get_task(assignee, &task.id).await
        }
    }
}

/// Advance every one of *this* instance's outstanding tasks once. Runs on a
/// periodic timer so tasks suspended on a slow provider operation (e.g.
/// "wait for deletion to finish", which can take hours) eventually get
/// resumed and completed without a request being in flight.
///
/// A task's owner is its assignee, so each instance sweeps only what it
/// picked up — and since `tasks` is partitioned on the assignee, this node's
/// whole backlog is a single query against the single instance its name
/// hashes to. No partition is enumerated and no other node's work is ever
/// read.
pub async fn sweep(app: &App) -> Result<()> {
    let node = &app.settings.node_name;
    for task in app.store.list_tasks(node).await? {
        advance(app, node, &task.id).await?;
    }
    Ok(())
}
