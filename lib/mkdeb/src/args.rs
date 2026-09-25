// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Command-line parsing. Hand-rolled to mirror the original shell loop:
//! options are processed left to right, `-h` exits immediately and any
//! unknown option is fatal (exit status 1).

pub const HELP: &str = r#"Copyright (C) The Usecode Authors (see AUTHORS)

mkdeb - Package files into a Debian (.deb) package

Usage: mkdeb [OPTIONS]

Options:
  -n, --name NAME           Package name (default: from .deb.json or directory name)
  -v, --version VERSION     Package version (default: from .deb.json or "0.1.0")
  -d, --description DESC    Package description
  -m, --maintainer EMAIL    Maintainer email (default: "maintainer@example.com")
  -a, --arch ARCH           Target architecture (default: auto-detect)
  -o, --output DIR          Output directory for .deb file (default: ./dist)
  -p, --prefix PREFIX       Installation prefix for docs (default: /usr)
  -s, --source DIR          Source directory (default: .)
  --tmp-dir DIR             Temp build directory (default: auto via mktemp; set to ./.tmp for local)
  --depends DEPS            Comma-separated list of dependencies
  --section SECTION         Package section (default: utils)
  --priority PRIORITY       Package priority (default: optional)
  --homepage URL            Project homepage URL
  --license LICENSE         License (default: HGL)
  -h, --help                Show this help message

.deb.json / .deb.jsonc:
  Place a .deb.json or .deb.jsonc file in the source directory to set
  package properties:
  { "name": "myapp", "version": "1.0.0", "description": "My app",
    "maintainer": "user@example.com", "depends": "libc6",
    "section": "utils", "priority": "optional",
    "homepage": "https://example.com", "license": "MIT",
    "root": "./pkg-root", "tmpDir": "./.tmp" }
  .deb.jsonc may contain comments (stripped via /usr/local/bin/jsonc).
  CLI arguments override values from these files.
  "root" specifies a directory whose contents are copied into the package
  root. E.g. if root is "./pkg-root" and it contains
  usr/bin/myapp, the .deb will install /usr/bin/myapp.
"#;

/// Values given on the command line. `None` means "not given".
#[derive(Debug, Default, PartialEq)]
pub struct Opts {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub maintainer: Option<String>,
    pub arch: Option<String>,
    pub output: Option<String>,
    pub prefix: Option<String>,
    pub source: Option<String>,
    pub tmp_dir: Option<String>,
    pub depends: Option<String>,
    pub section: Option<String>,
    pub priority: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
}

/// Returns `Ok(None)` when help was requested.
pub fn parse(args: &[String]) -> Result<Option<Opts>, String> {
    let mut o = Opts::default();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let slot = match arg.as_str() {
            "-n" | "--name" => &mut o.name,
            "-v" | "--version" => &mut o.version,
            "-d" | "--description" => &mut o.description,
            "-m" | "--maintainer" => &mut o.maintainer,
            "-a" | "--arch" => &mut o.arch,
            "-o" | "--output" => &mut o.output,
            "-p" | "--prefix" => &mut o.prefix,
            "-s" | "--source" => &mut o.source,
            "--tmp-dir" => &mut o.tmp_dir,
            "--depends" => &mut o.depends,
            "--section" => &mut o.section,
            "--priority" => &mut o.priority,
            "--homepage" => &mut o.homepage,
            "--license" => &mut o.license,
            "-h" | "--help" => return Ok(None),
            _ => return Err(format!("Unknown option: {arg}")),
        };
        let value = it
            .next()
            .ok_or_else(|| format!("Option {arg} requires an argument"))?;
        *slot = Some(value.clone());
    }
    Ok(Some(o))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Option<Opts>, String> {
        parse(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn parses_options() {
        let Ok(Some(o)) = p(&["-n", "x", "--tmp-dir", "./.tmp", "-v", "1.2"]) else {
            panic!()
        };
        assert_eq!(o.name.as_deref(), Some("x"));
        assert_eq!(o.version.as_deref(), Some("1.2"));
        assert_eq!(o.tmp_dir.as_deref(), Some("./.tmp"));
        assert_eq!(o.license, None);
    }

    #[test]
    fn help_and_errors() {
        assert_eq!(p(&["-h", "--bogus"]), Ok(None));
        assert_eq!(
            p(&["--bogus", "-h"]),
            Err("Unknown option: --bogus".to_string())
        );
        assert!(p(&["-n"]).is_err());
    }
}
