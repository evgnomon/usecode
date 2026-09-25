// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

use std::env;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use serde_json::{Map, Value};

/// Settings read from the nearest `.mongo.json`, walking up from the cwd.
#[derive(Debug)]
pub struct Config {
    pub container: String,
    pub network: String,
    pub image: String,
    pub password: String,
    pub user: String,
    pub port: String,
    pub host: String,
    pub database: String,
    pub data_dir: String,
    pub connection_string: String,
}

pub static CFG: LazyLock<Config> = LazyLock::new(load);

fn load() -> Config {
    let map = env::current_dir()
        .ok()
        .and_then(|cwd| {
            cwd.ancestors()
                .map(|d| d.join(".mongo.json"))
                .find(|p| p.is_file())
        })
        .map(|p| read(&p))
        .unwrap_or_default();
    from_map(&map)
}

fn read(path: &Path) -> Map<String, Value> {
    let data = match fs::read_to_string(path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: cannot read {}: {e}", path.display());
            std::process::exit(1);
        }
    };
    match serde_json::from_str::<Value>(&data) {
        Ok(Value::Object(m)) => m,
        Ok(_) => Map::new(),
        Err(e) => {
            eprintln!("Error: invalid {}: {e}", path.display());
            std::process::exit(1);
        }
    }
}

fn from_map(m: &Map<String, Value>) -> Config {
    let get = |key: &str, default: &str| -> String {
        match m.get(key) {
            None => default.to_string(),
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
        }
    };
    let home = env::var("HOME").unwrap_or_default();
    Config {
        container: get("container", "mongo1"),
        network: get("network", "mongonet"),
        image: get("image", "docker.io/library/mongo:7"),
        password: get("password", "mongopw"),
        user: get("user", "admin"),
        port: get("port", "27017"),
        host: get("host", "localhost"),
        database: get("database", "test"),
        data_dir: get("data_dir", &format!("{home}/.local/share/mongo/data")),
        connection_string: get("connection_string", ""),
    }
}

/// Like Python's `os.getenv(key, default)`: a set variable wins even if empty.
fn getenv(key: &str, fallback: &str) -> String {
    env::var(key).unwrap_or_else(|_| fallback.to_string())
}

/// Resolve connection string from env, config, or build it from parts.
pub fn connection_string() -> String {
    let cs = getenv("MONGO_CONNECTION_STRING", &CFG.connection_string);
    if !cs.is_empty() {
        return cs;
    }
    let host = getenv("MONGO_HOST", &CFG.host);
    let port = getenv("MONGO_PORT", &CFG.port);
    let user = getenv("MONGO_USER", &CFG.user);
    let password = getenv("MONGO_PASSWORD", &CFG.password);
    format!("mongodb://{user}:{password}@{host}:{port}/?authSource=admin")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_values() {
        let m: Map<String, Value> =
            serde_json::from_str(r#"{"port": 27018, "user": "u", "database": "d"}"#).unwrap();
        let c = from_map(&m);
        assert_eq!(c.port, "27018");
        assert_eq!(c.user, "u");
        assert_eq!(c.database, "d");
        assert_eq!(c.container, "mongo1");
        assert_eq!(c.connection_string, "");
    }
}
