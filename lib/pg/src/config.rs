use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

use serde::Deserialize;

/// Settings read from the nearest `.pg.json`, walking up from the cwd.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub container: String,
    pub network: String,
    pub image: String,
    pub password: String,
    pub user: String,
    pub port: u16,
    pub host: String,
    pub database: String,
    pub data_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        let home = env::var("HOME").unwrap_or_default();
        Self {
            container: "pg1".into(),
            network: "pgnet".into(),
            image: "docker.io/library/postgres:17".into(),
            password: "t6drtfyig7".into(),
            user: "admin".into(),
            port: 5432,
            host: "localhost".into(),
            database: "postgres".into(),
            data_dir: format!("{home}/.local/share/pg/data"),
        }
    }
}

pub static CFG: LazyLock<Config> = LazyLock::new(load);

fn load() -> Config {
    let Ok(cwd) = env::current_dir() else {
        return Config::default();
    };
    let found = cwd
        .ancestors()
        .map(|d| d.join(".pg.json"))
        .find(|p| p.is_file());
    found.map(|p| read(&p)).unwrap_or_default()
}

fn read(path: &PathBuf) -> Config {
    let data = fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&data).unwrap_or_else(|e| {
        eprintln!("Warning: ignoring invalid {}: {e}", path.display());
        Config::default()
    })
}

/// Environment variable if set and non-empty, else the fallback.
pub fn env_or(key: &str, fallback: &str) -> String {
    env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

pub fn host() -> String {
    env_or("PGHOST", &CFG.host)
}

pub fn port() -> String {
    env_or("PGPORT", &CFG.port.to_string())
}

pub fn user() -> String {
    env_or("PGUSER", &CFG.user)
}

pub fn password() -> String {
    env_or("PGPASSWORD", &CFG.password)
}

pub fn database() -> String {
    env_or("PGDATABASE", &CFG.database)
}
