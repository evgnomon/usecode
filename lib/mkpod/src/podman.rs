// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Interaction with podman.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::spec;

/// Temporary `.json` files removed on drop.
#[derive(Default)]
struct TempFiles(Vec<PathBuf>);

impl TempFiles {
    fn create(&mut self, content: &Value) -> Result<PathBuf> {
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir();
        let (path, mut file) = loop {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos());
            let seq = SEQ.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!("tmp{}{nanos:x}{seq}.json", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(f) => break (path, f),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e).context(format!("{}", path.display())),
            }
        };
        self.0.push(path.clone());
        write_json(&mut file, content)?;
        Ok(path)
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = fs::remove_file(p);
        }
    }
}

fn write_json(file: &mut File, v: &Value) -> Result<()> {
    file.write_all(serde_json::to_string_pretty(v)?.as_bytes())?;
    Ok(())
}

/// Write the manifests and run `podman kube play --replace`.
/// Returns true on success; failures are reported on stderr.
pub fn kube_play(pod_name: &str, pvcs: Vec<Value>, pod: Value, configmaps: &[Value]) -> bool {
    let mut tmp = TempFiles::default();
    let result = (|| -> Result<bool> {
        let pod_file = tmp.create(&spec::kube_list(pvcs, pod))?;
        let cm_files = configmaps
            .iter()
            .map(|cm| tmp.create(cm))
            .collect::<Result<Vec<_>>>()?;

        println!("Creating pod '{pod_name}'...");
        let mut cmd = Command::new("podman");
        cmd.args(["kube", "play", "--replace"]);
        for f in &cm_files {
            cmd.arg("--configmap").arg(f);
        }
        cmd.arg(&pod_file);
        let out = cmd.output().context("podman")?;
        match out.status.success() {
            true => {
                println!("Pod '{pod_name}' created successfully");
                if !out.stdout.is_empty() {
                    println!("{}", String::from_utf8_lossy(&out.stdout));
                }
                Ok(true)
            }
            false => {
                eprintln!("Failed to create pod '{pod_name}'");
                eprintln!("Error: {}", String::from_utf8_lossy(&out.stderr));
                Ok(false)
            }
        }
    })();
    result.unwrap_or_else(|e| {
        eprintln!("Error creating pod '{pod_name}': {e:#}");
        false
    })
}

/// (resource id, pod name) for all mkpod-managed podman pods.
pub fn managed_pods() -> Result<Vec<(String, String)>> {
    let label = format!("label={}", spec::RESOURCE_LABEL);
    let out = Command::new("podman")
        .args(["pod", "ls", "--filter", &label, "--format", "json"])
        .output()
        .context("podman")?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    match out.status.success() && !stdout.trim().is_empty() {
        true => Ok(spec::parse_managed(&stdout)),
        false => Ok(Vec::new()),
    }
}

/// `podman pod rm -f NAME`; Err carries podman's stderr.
pub fn remove_pod(name: &str) -> Result<std::result::Result<(), String>> {
    let out = Command::new("podman")
        .args(["pod", "rm", "-f", name])
        .output()
        .context("podman")?;
    Ok(match out.status.success() {
        true => Ok(()),
        false => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
    })
}
