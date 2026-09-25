// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! /tasks: the caller's in-flight background workflows.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};

use super::task_out;
use crate::App;
use crate::error::{AppError, AppResult};
use crate::extract::Client;
use crate::models::{TaskListOut, TaskOut};

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/tasks", get(list_tasks))
        .route("/tasks/{task_id}", get(get_task))
}

/// The caller's in-flight background tasks (e.g. the create_server /
/// delete_server workflows started by POST/DELETE /servers) that haven't
/// finished yet — a task disappears once done.
async fn list_tasks(
    State(app): State<Arc<App>>,
    Client(client): Client,
) -> AppResult<Json<TaskListOut>> {
    let tasks = app.store.list_user_tasks(&client.user_id).await?;
    Ok(Json(TaskListOut {
        tasks: tasks.into_iter().map(task_out).collect(),
    }))
}

async fn get_task(
    State(app): State<Arc<App>>,
    Client(client): Client,
    Path(task_id): Path<String>,
) -> AppResult<Json<TaskOut>> {
    // By id alone a task addresses nothing — `tasks` is partitioned on the
    // assignee — so this goes through the caller's own `user_tasks` index,
    // which is both what says where the row is and what proves it is theirs.
    app.store
        .get_user_task(&client.user_id, &task_id)
        .await?
        .map(|task| Json(task_out(task)))
        .ok_or_else(|| AppError::not_found("Task not found"))
}
