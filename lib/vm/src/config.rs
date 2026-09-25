// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! vm configuration, loaded from a simple `key: value` YAML subset.

use std::path::{Path, PathBuf};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct Config {
    pub base_image_path: String,
    pub vm_storage_path: String,
    pub image_name: String,
    pub cloud_init_template_path: String,
    /// Memory in KiB.
    pub default_memory: u64,
    pub default_vcpus: u32,
    /// Disk size in bytes.
    pub default_disk_size: u64,
    pub default_machine: String,
    pub max_retries: u32,
    pub username: String,
    pub ssh_key: String,
    pub identity_file: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            base_image_path: "/usr/share/vm/images".to_string(),
            vm_storage_path: "/var/lib/libvirt/vm".to_string(),
            image_name: "zamin".to_string(),
            cloud_init_template_path: "/usr/share/vm/images/cloud-init".to_string(),
            default_memory: 1024 * 1024, // 1GiB in KiB
            default_vcpus: 2,
            default_disk_size: 10 * 1024 * 1024 * 1024, // 10GiB in bytes
            default_machine: "pc-q35-10.0".to_string(),
            max_retries: 30,
            username: "vm".to_string(),
            ssh_key: String::new(),
            identity_file: "~/.ssh/id_ed25519".to_string(),
        }
    }
}

impl Config {
    pub fn new() -> Config {
        Config::default()
    }

    /// Loads a config file, leaving unspecified keys at their default values.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Config> {
        let contents = std::fs::read_to_string(path)?;
        Ok(Config::parse(&contents))
    }

    /// Parses YAML-style `key: value` lines; unknown keys and comments are ignored.
    pub fn parse(contents: &str) -> Config {
        let mut cfg = Config::default();

        for line in contents.lines() {
            let trimmed = line.trim_matches([' ', '\t', '\r']);
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("---") {
                continue;
            }

            // Split on the first ": " to handle values containing colons/spaces.
            let Some((key, value)) = trimmed.split_once(": ") else {
                continue;
            };
            let key = key.trim_matches([' ', '\t']);
            let value = value.trim_matches([' ', '\t']);

            match key {
                "base_image_path" => cfg.base_image_path = value.to_string(),
                "vm_storage_path" => cfg.vm_storage_path = value.to_string(),
                "cloud_init_template_path" => cfg.cloud_init_template_path = value.to_string(),
                "default_memory" => {
                    if let Ok(v) = value.parse() {
                        cfg.default_memory = v;
                    }
                }
                "default_vcpus" => {
                    if let Ok(v) = value.parse() {
                        cfg.default_vcpus = v;
                    }
                }
                "default_disk_size" => {
                    if let Ok(v) = value.parse() {
                        cfg.default_disk_size = v;
                    }
                }
                "default_machine" => cfg.default_machine = value.to_string(),
                "max_retries" => {
                    if let Ok(v) = value.parse() {
                        cfg.max_retries = v;
                    }
                }
                "username" => cfg.username = value.to_string(),
                "ssh_key" => cfg.ssh_key = value.to_string(),
                "identity_file" => cfg.identity_file = value.to_string(),
                "image_name" => cfg.image_name = value.to_string(),
                _ => {}
            }
        }

        cfg
    }

    /// Returns the first config file that exists: the system one, then the user one.
    pub fn default_config_path() -> Option<PathBuf> {
        let mut candidates = vec![PathBuf::from("/etc/vm/config.yaml")];
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(Path::new(&home).join(".config/vm/config.yaml"));
        }

        candidates.into_iter().find(|path| path.is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn parses_known_keys_and_keeps_defaults() {
        let cfg = Config::parse(
            "---\n# comment\nusername: hamed\nssh_key: ssh-ed25519 AAAA host: name\nmax_retries: 5\nbogus: x\nnot a pair\n",
        );

        assert_eq!(cfg.username, "hamed");
        assert_eq!(cfg.ssh_key, "ssh-ed25519 AAAA host: name");
        assert_eq!(cfg.max_retries, 5);
        assert_eq!(cfg.image_name, "zamin");
    }
}
