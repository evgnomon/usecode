//! A tiny command-line flag parser with the same surface uc daemon's users
//! already type: single-dash long flags (`-config PATH`, `-force`),
//! `-flag=value` too, and parsing that stops at the first non-flag
//! argument so `uc daemon forward NAME tcp 443` works unchanged.

use std::collections::HashMap;

use crate::error::{Error, Result};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Str,
    Int,
    Bool,
}

pub struct FlagSet {
    name: String,
    kinds: HashMap<String, Kind>,
    values: HashMap<String, String>,
    args: Vec<String>,
}

impl FlagSet {
    pub fn new(name: &str) -> FlagSet {
        FlagSet {
            name: name.to_string(),
            kinds: HashMap::new(),
            values: HashMap::new(),
            args: Vec::new(),
        }
    }

    pub fn string(&mut self, name: &str, default: &str) -> &mut Self {
        self.define(name, Kind::Str, default)
    }

    pub fn int(&mut self, name: &str, default: i64) -> &mut Self {
        self.define(name, Kind::Int, &default.to_string())
    }

    pub fn bool(&mut self, name: &str, default: bool) -> &mut Self {
        self.define(name, Kind::Bool, &default.to_string())
    }

    fn define(&mut self, name: &str, kind: Kind, default: &str) -> &mut Self {
        self.kinds.insert(name.to_string(), kind);
        self.values.insert(name.to_string(), default.to_string());
        self
    }

    /// Parse args, leaving everything from the first non-flag argument
    /// onwards as positional arguments.
    pub fn parse(&mut self, args: &[String]) -> Result<()> {
        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if arg == "--" {
                i += 1;
                break;
            }
            if arg.len() < 2 || !arg.starts_with('-') {
                break;
            }

            let body = arg.trim_start_matches('-');
            let (name, inline) = match body.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (body.to_string(), None),
            };

            let kind = *self
                .kinds
                .get(&name)
                .ok_or_else(|| Error(format!("flag provided but not defined: -{name}")))?;

            let value = match (kind, inline) {
                (_, Some(v)) => v,
                (Kind::Bool, None) => "true".to_string(),
                (_, None) => {
                    i += 1;
                    args.get(i)
                        .cloned()
                        .ok_or_else(|| Error(format!("flag needs an argument: -{name}")))?
                }
            };

            if kind == Kind::Int && value.parse::<i64>().is_err() {
                return Err(Error(format!(
                    "invalid value {value:?} for flag -{name}: parse error"
                )));
            }
            if kind == Kind::Bool && value.parse::<bool>().is_err() {
                return Err(Error(format!(
                    "invalid boolean value {value:?} for -{name}"
                )));
            }

            self.values.insert(name, value);
            i += 1;
        }

        self.args = args[i..].to_vec();
        Ok(())
    }

    pub fn get_str(&self, name: &str) -> String {
        self.values
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("{}: undefined flag -{name}", self.name))
    }

    pub fn get_int(&self, name: &str) -> i64 {
        self.get_str(name).parse().unwrap_or(0)
    }

    pub fn get_bool(&self, name: &str) -> bool {
        self.get_str(name).parse().unwrap_or(false)
    }

    pub fn nargs(&self) -> usize {
        self.args.len()
    }

    pub fn arg(&self, i: usize) -> String {
        self.args.get(i).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn separate_and_inline_values_both_work() {
        let mut fs = FlagSet::new("t");
        fs.string("config", "/etc/uc/config.toml")
            .int("listen-port", 51820);
        fs.parse(&argv(&["-config", "/tmp/c.toml", "--listen-port=1234"]))
            .unwrap();
        assert_eq!(fs.get_str("config"), "/tmp/c.toml");
        assert_eq!(fs.get_int("listen-port"), 1234);
    }

    #[test]
    fn bools_stand_alone() {
        let mut fs = FlagSet::new("t");
        fs.bool("force", false);
        fs.parse(&argv(&["-force"])).unwrap();
        assert!(fs.get_bool("force"));
    }

    #[test]
    fn parsing_stops_at_the_first_positional() {
        let mut fs = FlagSet::new("t");
        fs.string("config", "");
        fs.parse(&argv(&["NAME", "-config", "x"])).unwrap();
        assert_eq!(fs.nargs(), 3);
        assert_eq!(fs.arg(0), "NAME");
        assert_eq!(fs.get_str("config"), "");
    }

    #[test]
    fn unknown_flags_are_rejected() {
        let mut fs = FlagSet::new("t");
        assert!(fs.parse(&argv(&["-nope"])).is_err());
    }
}
