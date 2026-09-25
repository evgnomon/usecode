// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Env files under $PLAT_HOME: paths, defaults, backfill and port allocation.

use std::collections::hash_map::RandomState;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;

use crate::dotenv;
use crate::paths::Paths;

/// Tenant / namespace / pod selector (`-t`, `-n`, `-p`).
#[derive(Clone, Debug)]
pub struct Pod {
    pub tenant: String,
    pub namespace: String,
    pub pod: String,
}

fn io_fail(path: &Path, e: std::io::Error) -> ! {
    eprintln!("Error: {}: {e}", path.display());
    exit(1);
}

/// Set or replace KEY=VALUE in an env file (idempotent).
pub fn set_env_var(file: &Path, key: &str, value: &str) {
    let prefix = format!("{key}=");
    if let Ok(text) = fs::read_to_string(file) {
        let mut lines: Vec<String> = text.lines().map(String::from).collect();
        if let Some(line) = lines.iter_mut().find(|l| l.starts_with(&prefix)) {
            *line = format!("{key}={value}");
            let out = lines.join("\n") + "\n";
            if let Err(e) = fs::write(file, out) {
                io_fail(file, e);
            }
            return;
        }
    }
    append(file, &[(key.to_string(), value.to_string())]);
}

fn append(file: &Path, pairs: &[(String, String)]) {
    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .and_then(|mut fh| pairs.iter().try_for_each(|(k, v)| writeln!(fh, "{k}={v}")));
    if let Err(e) = result {
        io_fail(file, e);
    }
}

fn random_u64() -> u64 {
    let mut h = RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    h.finish()
}

/// All `*.env` files anywhere under `dir`.
fn env_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            env_files_recursive(&path, out);
        } else if entry.file_name().to_string_lossy().ends_with(".env") {
            out.push(path);
        }
    }
}

/// Pick a random port not currently used by any env file under PLAT_HOME.
pub fn allocate_port(plat_home: &Path) -> u16 {
    let mut files = Vec::new();
    env_files_recursive(plat_home, &mut files);
    let used: HashSet<u64> = files
        .iter()
        .filter_map(|f| dotenv::values(f).remove("PORT").flatten())
        .filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|p| p.parse().ok())
        .collect();
    loop {
        let candidate = 10000 + random_u64() % 55001;
        if !used.contains(&candidate) {
            return candidate as u16;
        }
    }
}

/// `$PLAT_HOME/<tenant>/<namespace>/<pod>/<service>.env`; exits if no pod.
pub fn env_path(paths: &Paths, pod: &Pod, service: &str) -> PathBuf {
    if pod.pod.is_empty() {
        eprintln!("Error: -p pod is required");
        exit(1);
    }
    paths
        .plat_home
        .join(&pod.tenant)
        .join(&pod.namespace)
        .join(&pod.pod)
        .join(format!("{service}.env"))
}

/// Template defaults for a pod, given the values already in its env file.
pub fn template(pod: &Pod, existing: &HashMap<String, String>) -> Vec<(String, String)> {
    let suffix = existing
        .get("DOMAIN_SUFFIX")
        .cloned()
        .unwrap_or_else(|| "local.zygote.run".into());
    let subdomain = existing
        .get("SUBDOMAIN")
        .cloned()
        .unwrap_or_else(|| pod.pod.clone());
    let domain = format!("{subdomain}.{}.{}.{suffix}", pod.namespace, pod.tenant);
    [
        ("TENANT", pod.tenant.clone()),
        ("NAMESPACE", pod.namespace.clone()),
        ("POD", pod.pod.clone()),
        ("STATUS", "down".into()),
        ("SUBDOMAIN", subdomain),
        ("DOMAIN_SUFFIX", suffix),
        ("DOMAIN", domain),
        ("CADDY_POD", "gateway".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// Ensure env file exists with all template defaults; backfill missing keys.
pub fn ensure_env(paths: &Paths, env_file: &Path, pod: &Pod) -> HashMap<String, String> {
    if let Some(parent) = env_file.parent()
        && let Err(e) = fs::create_dir_all(parent)
    {
        io_fail(parent, e);
    }
    let is_new = !env_file.is_file();
    let mut existing = if is_new {
        HashMap::new()
    } else {
        dotenv::read_env(env_file)
    };

    let mut added: Vec<(String, String)> = template(pod, &existing)
        .into_iter()
        .filter(|(k, _)| !existing.contains_key(k))
        .collect();
    if !existing.contains_key("PORT") {
        added.push(("PORT".into(), allocate_port(&paths.plat_home).to_string()));
    }

    if !added.is_empty() {
        append(env_file, &added);
        let keys: Vec<&str> = added.iter().map(|(k, _)| k.as_str()).collect();
        let verb = if is_new { "Created" } else { "Backfilled" };
        println!("{verb} {}: {}", env_file.display(), keys.join(", "));
        existing.extend(added);
    }
    existing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pod() -> Pod {
        Pod {
            tenant: "sys".into(),
            namespace: "main".into(),
            pod: "web".into(),
        }
    }

    #[test]
    fn template_defaults() {
        let t: HashMap<_, _> = template(&pod(), &HashMap::new()).into_iter().collect();
        assert_eq!(t["DOMAIN"], "web.main.sys.local.zygote.run");
        let existing = HashMap::from([
            ("SUBDOMAIN".to_string(), "api".to_string()),
            ("DOMAIN_SUFFIX".to_string(), "example.com".to_string()),
        ]);
        let t: HashMap<_, _> = template(&pod(), &existing).into_iter().collect();
        assert_eq!(t["DOMAIN"], "api.main.sys.example.com");
    }

    #[test]
    fn env_file_roundtrip() {
        let home = std::env::temp_dir().join(format!("plat-test-{}", std::process::id()));
        let paths = Paths {
            root: home.clone(),
            plat_home: home.clone(),
            templates: home.clone(),
        };
        let file = env_path(&paths, &pod(), "svc");
        let v = ensure_env(&paths, &file, &pod());
        let port: u32 = v["PORT"].parse().unwrap();
        assert!((10000..=65000).contains(&port));
        assert_eq!(v["STATUS"], "down");

        set_env_var(&file, "STATUS", "up");
        set_env_var(&file, "EXTRA", "1");
        let r = dotenv::read_env(&file);
        assert_eq!(r["STATUS"], "up");
        assert_eq!(r["EXTRA"], "1");
        assert_eq!(r["PORT"], v["PORT"]);

        fs::write(&file, "PORT=12345\nSUBDOMAIN=x\n").unwrap();
        let v = ensure_env(&paths, &file, &pod());
        assert_eq!(v["PORT"], "12345");
        assert_eq!(v["DOMAIN"], "x.main.sys.local.zygote.run");
        fs::remove_dir_all(&home).unwrap();
    }
}
