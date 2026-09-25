// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Run the AI model (llama-server) as a container on the usecode-agent-api
//! host, through the configured container CLI (podman or docker).

use std::path::PathBuf;
use std::process::Output;

use serde_json::{Map, Value};
use tokio::process::Command;

use crate::config::Settings;
use crate::model_config::resolve;

const CONTAINER_NAME: &str = "usecode-agent-model";
const INTERNAL_PORT: u16 = 8080;

// Local image built from MODEL_CONTAINERFILE, layering extra tooling (curl,
// bash, procps, ...) on top of whichever upstream llama.cpp image is
// selected. Rebuilt every time the model is started so it always tracks the
// chosen base image.
const MODEL_IMAGE: &str = "usecode-agent-model:local";
const MODEL_CONTAINERFILE: &str = include_str!("../model_image/Containerfile");

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("{0}")]
    Invalid(String),
    #[error("`{command}` failed ({code}): {output}")]
    Process {
        command: String,
        code: i32,
        output: String,
    },
}

async fn run(settings: &Settings, args: &[String]) -> Result<Output, ModelError> {
    let command = || format!("{} {}", settings.model_container_cli, args.join(" "));
    Command::new(&settings.model_container_cli)
        .args(args)
        .output()
        .await
        .map_err(|err| ModelError::Process {
            command: command(),
            code: -1,
            output: err.to_string(),
        })
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn check(settings: &Settings, args: &[String], output: Output) -> Result<Output, ModelError> {
    if output.status.success() {
        return Ok(output);
    }
    let stderr = text(&output.stderr);
    let detail = if stderr.is_empty() {
        text(&output.stdout)
    } else {
        stderr
    };
    Err(ModelError::Process {
        command: format!("{} {}", settings.model_container_cli, args.join(" ")),
        code: output.status.code().unwrap_or(-1),
        output: detail.trim().to_string(),
    })
}

/// The build context: the Containerfile is compiled into the binary and
/// written out next to nothing else, since it copies no files in.
async fn build_context() -> Result<PathBuf, ModelError> {
    let dir = std::env::temp_dir().join("usecode-agent-model");
    let write = async {
        tokio::fs::create_dir_all(&dir).await?;
        tokio::fs::write(dir.join("Containerfile"), MODEL_CONTAINERFILE).await
    };
    write.await.map_err(|err| ModelError::Process {
        command: "write model Containerfile".to_string(),
        code: -1,
        output: err.to_string(),
    })?;
    Ok(dir)
}

fn build_args(config: &Map<String, Value>, context: &std::path::Path) -> Vec<String> {
    vec![
        "build".into(),
        "--build-arg".into(),
        format!("BASE_IMAGE={}", as_arg(&config["image"])),
        "-t".into(),
        MODEL_IMAGE.into(),
        "-f".into(),
        context.join("Containerfile").display().to_string(),
        context.display().to_string(),
    ]
}

fn as_arg(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn run_args(config: &Map<String, Value>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "run".into(),
        "-d".into(),
        "--replace".into(),
        "--name".into(),
        CONTAINER_NAME.into(),
        "-p".into(),
        format!(
            "{}:{}:{INTERNAL_PORT}",
            as_arg(&config["host"]),
            as_arg(&config["port"])
        ),
    ];
    if as_arg(&config["device"]).to_lowercase().contains("vulkan") {
        args.extend(["--device".into(), "/dev/dri".into()]);
    }
    args.push(MODEL_IMAGE.into());
    for (flag, field) in [
        ("-hf", "hf_repo"),
        ("--device", "device"),
        ("-ngl", "ngl"),
        ("--alias", "alias"),
        ("-c", "ctx_size"),
    ] {
        args.extend([flag.to_string(), as_arg(&config[field])]);
    }
    args.extend([
        "--host".into(),
        "0.0.0.0".into(),
        "--port".into(),
        INTERNAL_PORT.to_string(),
    ]);
    args
}

/// Resolve overrides, (re)build the model image, and (re)start the
/// container. Returns the resolved configuration.
pub async fn start(
    settings: &Settings,
    overrides: &Map<String, Value>,
) -> Result<Map<String, Value>, ModelError> {
    let config = resolve(overrides).map_err(ModelError::Invalid)?;
    let context = build_context().await?;
    let build = build_args(&config, &context);
    check(settings, &build, run(settings, &build).await?)?;
    let start = run_args(&config);
    check(settings, &start, run(settings, &start).await?)?;
    Ok(config)
}

/// Stop and remove the model container. No-op if it isn't running.
pub async fn stop(settings: &Settings) -> Result<(), ModelError> {
    let args: Vec<String> = vec!["rm".into(), "-f".into(), CONTAINER_NAME.into()];
    let output = run(settings, &args).await?;
    if !output.status.success()
        && text(&output.stderr)
            .to_lowercase()
            .contains("no such container")
    {
        return Ok(());
    }
    check(settings, &args, output).map(|_| ())
}

/// Current running state of the model container: (running, state).
pub async fn status(settings: &Settings) -> (bool, Option<Value>) {
    let args: Vec<String> = vec![
        "inspect".into(),
        CONTAINER_NAME.into(),
        "--format".into(),
        "{{json .State}}".into(),
    ];
    match run(settings, &args).await {
        Ok(output) if output.status.success() => {
            let raw = text(&output.stdout);
            let raw = raw.trim();
            let state: Value = serde_json::from_str(if raw.is_empty() { "{}" } else { raw })
                .unwrap_or_else(|_| Value::Object(Map::new()));
            let running = state
                .get("Running")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            (running, Some(state))
        }
        _ => (false, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_args_reproduce_the_default_command() {
        let config = resolve(&Map::new()).unwrap();
        assert_eq!(
            run_args(&config).join(" "),
            "run -d --replace --name usecode-agent-model -p 127.0.0.1:8080:8080 \
             --device /dev/dri usecode-agent-model:local -hf ggml-org/Qwen3-0.6B-GGUF:Q4_0 \
             --device Vulkan0 -ngl 99 --alias local-model -c 32768 --host 0.0.0.0 --port 8080"
        );
    }
}
