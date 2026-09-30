// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `~/.ssh/config.d/<name>` entries for the servers the bot creates, so a new
//! server is one `ssh <name>` away. Assumes `~/.ssh/config` has
//! `Include config.d/*`.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;

use crate::client::Client;
use crate::config::Settings;

/// First line of every entry the bot writes. A file without it belongs to
/// someone else and is never overwritten or removed.
const MARKER: &str = "# Written by uc-agent-mcp";

/// The key `create_server` installs on every server (see
/// `client::local_ssh_public_key`).
const IDENTITY_FILE: &str = "~/.ssh/aurora";

pub const DEFAULT_USER: &str = "root";

/// How often, and for how long, to wait on a create_server task before
/// giving up on writing the entry.
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const POLL_ATTEMPTS: u32 = 180;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub host: String,
    pub hostname: String,
    pub user: String,
    pub proxy_jump: Option<String>,
    pub server_id: Option<String>,
}

impl Entry {
    pub fn render(&self) -> String {
        let mut text = MARKER.to_string();
        if let Some(id) = &self.server_id {
            text.push_str(&format!(" for usecode server {id}"));
        }
        text.push_str(&format!(
            "\nHost {}\n  Hostname {}\n  IdentityFile {IDENTITY_FILE}\n  User {}\n",
            self.host, self.hostname, self.user
        ));
        if let Some(jump) = &self.proxy_jump {
            text.push_str(&format!("  ProxyJump {jump}\n"));
        }
        text
    }
}

pub fn config_dir(settings: &Settings) -> PathBuf {
    let raw = settings
        .ssh_config_dir
        .clone()
        .unwrap_or_else(|| "~/.ssh/config.d".to_string());
    crate::compose::expand_user(&raw)
}

/// A server name is used as both the Host alias and the file name, so it
/// must be a plain word.
fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !host.starts_with('.')
}

fn ours(path: &Path) -> io::Result<bool> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text.starts_with(MARKER)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error),
    }
}

/// Write `entry` to `<dir>/<host>`, refusing to clobber a file the bot
/// didn't write.
pub fn write(dir: &Path, entry: &Entry) -> io::Result<PathBuf> {
    if !valid_host(&entry.host) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{}' can't be used as an ssh Host alias", entry.host),
        ));
    }
    let path = dir.join(&entry.host);
    if !ours(&path)? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "{} exists and wasn't written by uc-agent-mcp",
                path.display()
            ),
        ));
    }
    std::fs::create_dir_all(dir)?;
    std::fs::write(&path, entry.render())?;
    Ok(path)
}

/// Remove `<dir>/<host>` if the bot wrote it. Missing is fine.
pub fn remove(dir: &Path, host: &str) -> io::Result<Option<PathBuf>> {
    if !valid_host(host) {
        return Ok(None);
    }
    let path = dir.join(host);
    if !path.exists() || !ours(&path)? {
        return Ok(None);
    }
    std::fs::remove_file(&path)?;
    Ok(Some(path))
}

/// The entry for a server as the API reports it, once it has an address.
pub fn entry_for(server: &Value, user: &str, proxy_jump: Option<&str>) -> Option<Entry> {
    let text = |key: &str| server.get(key).and_then(Value::as_str).map(str::to_string);
    let hostname = text("public_ip4").or_else(|| text("public_ip6"))?;
    Some(Entry {
        host: text("name")?,
        hostname,
        user: user.to_string(),
        proxy_jump: proxy_jump.map(str::to_string),
        server_id: text("id"),
    })
}

/// Wait for a create_server task to finish (it 404s once done), then write
/// the entry for the server it made. Runs detached from the tool call, so
/// failures are only logged to stderr; `write_ssh_config` covers a retry.
pub async fn write_when_ready(
    client: Client,
    dir: PathBuf,
    task_id: String,
    name: String,
    user: String,
    proxy_jump: Option<String>,
    api_key: Option<String>,
) {
    for _ in 0..POLL_ATTEMPTS {
        tokio::time::sleep(POLL_INTERVAL).await;
        match client.get_task(&task_id, api_key.clone()).await {
            Err(error) if error.status_code == 404 => break,
            _ => continue,
        }
    }
    let servers = match client.list_servers(api_key).await {
        Ok(body) => body,
        Err(error) => return eprintln!("uc-agent-mcp: ssh config for {name}: {error}"),
    };
    let entry = servers
        .get("servers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|server| server.get("name").and_then(Value::as_str) == Some(&name))
        .find_map(|server| entry_for(server, &user, proxy_jump.as_deref()));
    match entry.map(|entry| write(&dir, &entry)) {
        Some(Ok(_)) => {}
        Some(Err(error)) => eprintln!("uc-agent-mcp: ssh config for {name}: {error}"),
        None => eprintln!("uc-agent-mcp: ssh config for {name}: server has no address yet"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("uc-agent-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn entry(jump: Option<&str>) -> Entry {
        entry_for(
            &json!({"id": "s1", "name": "web", "public_ip4": "1.2.3.4"}),
            DEFAULT_USER,
            jump,
        )
        .unwrap()
    }

    #[test]
    fn renders_user_key_and_bastion() {
        assert_eq!(
            entry(Some("shadow")).render(),
            "# Written by uc-agent-mcp for usecode server s1\n\
             Host web\n  Hostname 1.2.3.4\n  IdentityFile ~/.ssh/aurora\n  User root\n  ProxyJump shadow\n"
        );
        assert!(!entry(None).render().contains("ProxyJump"));
    }

    #[test]
    fn no_address_means_no_entry() {
        assert!(entry_for(&json!({"name": "web"}), DEFAULT_USER, None).is_none());
    }

    #[test]
    fn never_touches_files_it_did_not_write() {
        let dir = scratch("foreign");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("web"), "Host web\n").unwrap();
        assert!(write(&dir, &entry(None)).is_err());
        assert_eq!(remove(&dir, "web").unwrap(), None);
        assert!(dir.join("web").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn writes_rewrites_and_removes_its_own() {
        let dir = scratch("own");
        let path = write(&dir, &entry(None)).unwrap();
        write(&dir, &entry(Some("shadow"))).unwrap();
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("ProxyJump shadow")
        );
        assert_eq!(remove(&dir, "web").unwrap(), Some(path.clone()));
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_names_that_are_not_a_plain_word() {
        assert!(!valid_host("../evil"));
        assert!(!valid_host("a b"));
        assert!(valid_host("web-1.fsn"));
    }
}
