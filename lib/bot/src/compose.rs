// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Drive the `deploy/compose.yml` stack (podman-compose or docker compose).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use thiserror::Error;
use tokio::process::Command;

use crate::config::Settings;

/// lib/bot -> repo root.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate lives at <repo>/lib/bot")
        .to_path_buf()
}

#[derive(Debug, Clone, Error)]
#[error("`{}` failed ({returncode}): {}", .command.join(" "), .output.trim())]
pub struct ComposeError {
    pub command: Vec<String>,
    pub returncode: i32,
    pub output: String,
}

pub type ComposeResult<T> = Result<T, ComposeError>;

fn expand_user(raw: &str) -> PathBuf {
    match raw.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => Path::new(&home).join(rest),
            None => PathBuf::from(raw),
        },
        None => PathBuf::from(raw),
    }
}

pub fn compose_file(settings: &Settings) -> PathBuf {
    match &settings.compose_file {
        Some(path) => {
            let path = expand_user(path);
            std::fs::canonicalize(&path).unwrap_or(path)
        }
        None => repo_root().join("deploy").join("compose.yml"),
    }
}

pub fn compose_command(settings: &Settings) -> Vec<String> {
    let path = compose_file(settings).to_string_lossy().into_owned();
    match settings.container_cli.as_str() {
        "docker" => vec!["docker".into(), "compose".into(), "-f".into(), path],
        _ => vec!["podman-compose".into(), "-f".into(), path],
    }
}

/// Run a compose subcommand and return its stdout, or the failure as a
/// `ComposeError` — including the tooling not being installed at all.
async fn run(settings: &Settings, args: &[&str]) -> ComposeResult<Output> {
    let mut command: Vec<String> = compose_command(settings);
    command.extend(args.iter().map(|arg| arg.to_string()));

    let working_dir = compose_file(settings)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(repo_root);

    let output = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(working_dir)
        .output()
        .await
        .map_err(|error| ComposeError {
            command: command.clone(),
            returncode: -1,
            output: error.to_string(),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        let returncode = output.status.code().unwrap_or(-1);
        let output = if stderr.is_empty() { stdout } else { stderr };
        return Err(ComposeError {
            command,
            returncode,
            output,
        });
    }
    Ok(Output { stdout, stderr })
}

pub struct Output {
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    fn to_value(&self) -> Map<String, Value> {
        let mut map = Map::new();
        map.insert("stdout".to_string(), json!(self.stdout));
        map.insert("stderr".to_string(), json!(self.stderr));
        map
    }
}

/// All service names defined in the compose file.
pub async fn services(settings: &Settings) -> ComposeResult<Vec<String>> {
    let output = run(settings, &["config", "--services"]).await?;
    Ok(output
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

fn parse_ps_json(text: &str) -> Vec<Value> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Array(entries)) => entries,
        Ok(entry) => vec![entry],
        // docker compose emits JSON-lines rather than a single array/object.
        Err(_) => text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect(),
    }
}

fn service_name(entry: &Value) -> Option<&str> {
    entry
        .get("Service")
        .and_then(Value::as_str)
        .or_else(|| {
            entry
                .get("Labels")?
                .get("com.docker.compose.service")?
                .as_str()
        })
        .filter(|name| !name.is_empty())
}

/// Names of services from the compose file with a currently running container.
pub async fn running_services(settings: &Settings) -> ComposeResult<Vec<String>> {
    let output = run(settings, &["ps", "--format", "json"]).await?;
    Ok(parse_ps_json(&output.stdout)
        .iter()
        .filter(|entry| entry.get("State").and_then(Value::as_str) == Some("running"))
        .filter_map(service_name)
        .map(str::to_string)
        .collect())
}

/// True if every service defined in the compose file has a running container.
pub async fn is_running(settings: &Settings) -> ComposeResult<bool> {
    let expected = services(settings).await?;
    let running = running_services(settings).await?;
    Ok(!expected.is_empty() && expected.iter().all(|service| running.contains(service)))
}

/// Build and bring the compose stack up in the background.
///
/// `--force-recreate` is required because compose otherwise reuses an
/// existing (stopped) container tied to the old image even when `--build`
/// produced a newer one under the same tag.
pub async fn start(settings: &Settings) -> ComposeResult<Map<String, Value>> {
    let output = run(settings, &["up", "-d", "--build", "--force-recreate"]).await?;
    Ok(output.to_value())
}

/// Stop and remove the compose stack's containers.
pub async fn stop(settings: &Settings) -> ComposeResult<Map<String, Value>> {
    let output = run(settings, &["down"]).await?;
    Ok(output.to_value())
}

/// Shell commands the user can run to follow logs for each service.
pub async fn logs_commands(settings: &Settings) -> ComposeResult<Value> {
    let base = compose_command(settings).join(" ");
    let mut commands = Map::new();
    for service in services(settings).await? {
        commands.insert(service.clone(), json!(format!("{base} logs -f {service}")));
    }
    commands.insert("all".to_string(), json!(format!("{base} logs -f")));
    Ok(Value::Object(commands))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_compose_file_is_deploy_compose_yml() {
        let settings = Settings::default();
        assert!(compose_file(&settings).ends_with("deploy/compose.yml"));
    }

    #[test]
    fn container_cli_picks_the_tooling() {
        let mut settings = Settings::default();
        assert_eq!(compose_command(&settings)[0], "podman-compose");
        settings.container_cli = "docker".to_string();
        assert_eq!(&compose_command(&settings)[..2], &["docker", "compose"]);
    }

    #[test]
    fn ps_json_reads_arrays_and_json_lines() {
        let array = parse_ps_json(r#"[{"Service": "api-1", "State": "running"}]"#);
        assert_eq!(service_name(&array[0]), Some("api-1"));

        let lines = parse_ps_json("{\"Service\": \"a\"}\n{\"Service\": \"b\"}");
        assert_eq!(lines.len(), 2);

        assert!(parse_ps_json("  ").is_empty());
    }

    #[test]
    fn ps_json_falls_back_to_the_compose_label() {
        let entries = parse_ps_json(
            r#"[{"Labels": {"com.docker.compose.service": "caddy"}, "State": "running"}]"#,
        );
        assert_eq!(service_name(&entries[0]), Some("caddy"));
    }
}
