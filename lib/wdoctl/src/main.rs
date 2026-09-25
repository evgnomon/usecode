// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Run doctl with DIGITALOCEAN_ACCESS_TOKEN taken from the current
//! repository's secrets (`getsecret <repofqn>` -> `.doctl.prod`).

mod repofqn;

use serde_json::Value;
use std::env;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio, exit};

/// jq -r output for one value.
fn raw(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

/// `jq -r '.doctl.prod'` over a stream of JSON values, captured like `$(...)`.
fn doctl_prod(input: &[u8]) -> String {
    let mut out = String::new();
    for item in serde_json::Deserializer::from_slice(input).into_iter::<Value>() {
        let v = match item {
            Ok(v) => v,
            Err(e) => {
                eprintln!("jq: error (at <stdin>): {e}");
                break;
            }
        };
        let doctl = match &v {
            Value::Object(m) => m.get("doctl").cloned().unwrap_or(Value::Null),
            Value::Null => Value::Null,
            _ => {
                eprintln!("jq: error (at <stdin>): Cannot index value with \"doctl\"");
                break;
            }
        };
        let prod = match &doctl {
            Value::Object(m) => m.get("prod").cloned().unwrap_or(Value::Null),
            Value::Null => Value::Null,
            _ => {
                eprintln!("jq: error (at <stdin>): Cannot index value with \"prod\"");
                break;
            }
        };
        out.push_str(&raw(&prod));
        out.push('\n');
    }
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

fn main() {
    let secrets = match Command::new("getsecret")
        .arg(repofqn::repofqn())
        .stdin(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
    {
        Ok(o) => o.stdout,
        Err(e) => {
            eprintln!("wdoctl: getsecret: {e}");
            Vec::new()
        }
    };
    let err = Command::new("doctl")
        .env("DIGITALOCEAN_ACCESS_TOKEN", doctl_prod(&secrets))
        .args(env::args_os().skip(1))
        .exec();
    eprintln!("wdoctl: doctl: {err}");
    exit(if err.kind() == io::ErrorKind::NotFound {
        127
    } else {
        126
    });
}

#[cfg(test)]
mod tests {
    use super::doctl_prod;

    #[test]
    fn extracts_token() {
        assert_eq!(doctl_prod(br#"{"doctl":{"prod":"tok"}}"#), "tok");
        assert_eq!(doctl_prod(br#"{"other":1}"#), "null");
        assert_eq!(doctl_prod(b""), "");
        assert_eq!(doctl_prod(b"Vault file not found: x"), "");
    }
}
