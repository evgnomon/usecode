// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! A flag can always be given three ways: on the command line, in the
//! environment, or in the user config. `--no-shell` is also `UC_NO_SHELL=1`
//! and, in `config.yaml`,
//!
//! ```yaml
//! uc:
//!   no_shell: true
//! ```
//!
//! The command line wins over the environment, and the environment over the
//! config. A value of `0`, `false`, `no`, `off` or nothing turns a flag off,
//! anything else on, so `UC_NO_SHELL=0` undoes a `no_shell: true`.

use crate::configure::vars::read_user_config;
use crate::secret::user_repo;
use serde_yaml::Value;
use std::sync::OnceLock;

/// Whether `flag` (`--no-shell`) is on: given in `args`, else set in
/// `UC_NO_SHELL`, else under `uc: no_shell:` in the user config.
pub fn on(args: &[String], flag: &str) -> bool {
    if args.iter().any(|a| a == flag) {
        return true;
    }
    let name = flag.trim_start_matches('-').replace('-', "_");
    if let Ok(value) = std::env::var(format!("UC_{}", name.to_uppercase())) {
        return truthy(&value);
    }
    match config().get(name.as_str()) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => truthy(s),
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        _ => false,
    }
}

/// The value of `flag` (`--roles`): `given` on the command line, else
/// `UC_ROLES`, else `uc: roles:` in the user config.
pub fn value(given: Option<String>, flag: &str) -> Option<String> {
    if given.is_some() {
        return given;
    }
    let name = flag.trim_start_matches('-').replace('-', "_");
    if let Ok(value) = std::env::var(format!("UC_{}", name.to_uppercase())) {
        return Some(value).filter(|v| !v.is_empty());
    }
    match config().get(name.as_str()) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn truthy(value: &str) -> bool {
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "no" | "off"
    )
}

/// The `uc:` section of the user config, read once; empty without one.
fn config() -> &'static serde_yaml::Mapping {
    static CONFIG: OnceLock<serde_yaml::Mapping> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let file = user_repo("config/config.yaml");
        if !file.is_file() {
            return Default::default();
        }
        match read_user_config(&file) {
            Ok(config) => match config.get("uc") {
                Some(Value::Mapping(uc)) => uc.clone(),
                _ => Default::default(),
            },
            Err(e) => {
                eprintln!("uc: ignoring {}: {e:#}", file.display());
                Default::default()
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthy_values() {
        for v in ["1", "true", "YES", "on", "y"] {
            assert!(truthy(v), "{v}");
        }
        for v in ["", " ", "0", "false", "No", "off"] {
            assert!(!truthy(v), "{v}");
        }
    }

    #[test]
    fn command_line_wins() {
        assert!(on(&["--ask".into()], "--ask"));
    }
}
