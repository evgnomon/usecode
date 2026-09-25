// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Background execution of a run, capturing stdout/stderr to files.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::Stdio;

use super::Db;
use crate::util::now_iso;

pub struct Job {
    pub run_id: String,
    pub tenant_name: String,
    pub namespace: String,
    pub job_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub output_dir: PathBuf,
    pub env: BTreeMap<String, String>,
    pub working_dir: Option<String>,
}

fn update(db: &Db, run_id: &str, f: impl FnOnce(&mut super::RunInfo)) {
    let mut runs = db.lock().expect("runs lock");
    if let Some(r) = runs.iter_mut().find(|r| r.run_id == run_id) {
        f(r);
    }
}

/// Execute a process and capture stdout/stderr to files.
pub async fn execute(db: Db, job: Job) {
    let stdout_path = job.output_dir.join("stdout.log");
    let stderr_path = job.output_dir.join("stderr.log");

    if let Err(e) = std::fs::create_dir_all(&job.output_dir) {
        update(&db, &job.run_id, |r| {
            r.status = "error".into();
            r.error = Some(e.to_string());
            r.finished_at = Some(now_iso());
        });
        return;
    }

    let started_at = now_iso();
    update(&db, &job.run_id, |r| {
        r.status = "running".into();
        r.started_at = Some(started_at.clone());
    });

    match spawn_and_wait(&job, &stdout_path, &stderr_path).await {
        Ok(exit_code) => {
            let finished_at = now_iso();
            update(&db, &job.run_id, |r| {
                r.status = match exit_code {
                    0 => "completed",
                    _ => "failed",
                }
                .into();
                r.exit_code = Some(exit_code);
                r.finished_at = Some(finished_at.clone());
            });
            let metadata = format!(
                "run_id: {}\ncommand: {} {}\nexit_code: {}\nstarted_at: {}\nfinished_at: {}\n",
                job.run_id,
                job.command,
                job.args.join(" "),
                exit_code,
                started_at,
                finished_at
            );
            let _ = std::fs::write(job.output_dir.join("metadata.txt"), metadata);
        }
        Err(e) => {
            update(&db, &job.run_id, |r| {
                r.status = "error".into();
                r.error = Some(e.to_string());
                r.finished_at = Some(now_iso());
            });
            let _ = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&stderr_path)
                .and_then(|mut f| write!(f, "\n[SYSTEM ERROR]: {e}\n"));
        }
    }
}

async fn spawn_and_wait(
    job: &Job,
    stdout_path: &std::path::Path,
    stderr_path: &std::path::Path,
) -> std::io::Result<i32> {
    let stdout_file = File::create(stdout_path)?;
    let stderr_file = File::create(stderr_path)?;
    let output_dir = job.output_dir.to_string_lossy().to_string();
    let mut cmd = tokio::process::Command::new(&job.command);
    cmd.args(&job.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .env("WORKD_RUN_ID", &job.run_id)
        .env("WORKD_TENANT_NAME", &job.tenant_name)
        .env("WORKD_NAMESPACE", &job.namespace)
        .env("WORKD_JOB_NAME", &job.job_name)
        .env("WORKD_OUTPUT_DIR", &output_dir)
        .env("WORKD_STDOUT_PATH", stdout_path)
        .env("WORKD_STDERR_PATH", stderr_path)
        .envs(&job.env);
    if let Some(dir) = &job.working_dir {
        cmd.current_dir(dir);
    }
    let status = cmd.spawn()?.wait().await?;
    Ok(status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(0)))
}
