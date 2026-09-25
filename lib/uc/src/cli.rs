// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Option parsing, password prompting and safe file replacement shared by the
//! `uc-encrypt` and `uc-decrypt` binaries.

use crate::{Error, Result};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

/// The flags both subcommands understand, plus the single file they act on.
#[derive(Debug, Default)]
pub struct Options {
    /// `-c`: write the result to stdout instead of a file.
    pub to_stdout: bool,
    /// `-f`: replace an existing output file.
    pub force: bool,
    /// `-h`: print usage and exit successfully.
    pub help: bool,
    pub file: PathBuf,
}

/// Parses the short options in the order `getopts` would, stopping at the first
/// non-option argument. `allowed` lists the flag letters the caller accepts.
pub fn parse(args: &[String], allowed: &str) -> Result<Options> {
    let mut opts = Options::default();
    let mut rest = args.iter();
    let mut operands: Vec<&String> = Vec::new();

    for arg in rest.by_ref() {
        if arg == "--" {
            break;
        }
        if arg == "--help" {
            opts.help = true;
            return Ok(opts);
        }
        if !(arg.starts_with('-') && arg.len() > 1) {
            operands.push(arg);
            break;
        }
        for flag in arg.chars().skip(1) {
            if flag != 'h' && !allowed.contains(flag) {
                return Err(Error::Usage(format!("unknown option -{flag}")));
            }
            match flag {
                'c' => opts.to_stdout = true,
                'f' => opts.force = true,
                'h' => opts.help = true,
                _ => return Err(Error::Usage(format!("unknown option -{flag}"))),
            }
        }
        if opts.help {
            return Ok(opts);
        }
    }
    operands.extend(rest);

    match operands.len() {
        1 => {
            opts.file = PathBuf::from(operands[0]);
            Ok(opts)
        }
        0 => Err(Error::Usage("expected a filename".to_string())),
        n => Err(Error::Usage(format!("expected one filename, got {n}"))),
    }
}

/// Reads the whole file, reporting the path in any error.
pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    if !path.is_file() {
        return Err(Error::Format(format!(
            "'{}' does not exist or is not a regular file",
            path.display()
        )));
    }
    fs::read(path).map_err(|err| Error::Io(format!("reading '{}'", path.display()), err))
}

/// Refuses to clobber an existing output file unless `-f` was given.
pub fn check_output(path: &Path, force: bool, hint: &str) -> Result<()> {
    if path.exists() && !force {
        return Err(Error::Format(format!(
            "'{}' already exists ({hint})",
            path.display()
        )));
    }
    Ok(())
}

/// Writes `contents` to a sibling temporary file and renames it into place, so
/// an interrupted write never leaves a half-written file behind.
pub fn write_atomic(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let name = path
        .file_name()
        .ok_or_else(|| Error::Format(format!("'{}' is not a file path", path.display())))?;
    let mut tmp_name = name.to_os_string();
    tmp_name.push(format!(".uc.{}", std::process::id()));
    let tmp = match dir {
        Some(dir) => dir.join(&tmp_name),
        None => PathBuf::from(&tmp_name),
    };

    let write = |tmp: &Path| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode)
            .open(tmp)?;
        file.write_all(contents)?;
        file.sync_all()
    };

    if let Err(err) = write(&tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(Error::Io(format!("writing '{}'", tmp.display()), err));
    }
    fs::rename(&tmp, path).map_err(|err| {
        let _ = fs::remove_file(&tmp);
        Error::Io(format!("replacing '{}'", path.display()), err)
    })
}

/// Prompts on the terminal for a password that is never echoed.
pub fn prompt_password(prompt: &str) -> Result<Zeroizing<String>> {
    rpassword::prompt_password(prompt)
        .map(Zeroizing::new)
        .map_err(|err| Error::Io("reading the password from the terminal".to_string(), err))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_a_bare_filename() {
        let opts = parse(&args(&["secrets.txt"]), "f").unwrap();
        assert_eq!(opts.file, PathBuf::from("secrets.txt"));
        assert!(!opts.force);
    }

    #[test]
    fn parses_bundled_flags() {
        let opts = parse(&args(&["-cf", "secrets.txt.asc"]), "cf").unwrap();
        assert!(opts.to_stdout);
        assert!(opts.force);
    }

    #[test]
    fn rejects_a_flag_the_subcommand_does_not_take() {
        assert!(matches!(
            parse(&args(&["-c", "x"]), "f"),
            Err(Error::Usage(_))
        ));
    }

    #[test]
    fn stops_option_parsing_at_the_double_dash() {
        let opts = parse(&args(&["--", "-weird-name"]), "cf").unwrap();
        assert_eq!(opts.file, PathBuf::from("-weird-name"));
    }

    #[test]
    fn requires_exactly_one_file() {
        assert!(matches!(parse(&args(&[]), "cf"), Err(Error::Usage(_))));
        assert!(matches!(
            parse(&args(&["a", "b"]), "cf"),
            Err(Error::Usage(_))
        ));
    }

    #[test]
    fn help_wins_over_a_missing_filename() {
        assert!(parse(&args(&["-h"]), "cf").unwrap().help);
        assert!(parse(&args(&["--help"]), "cf").unwrap().help);
    }
}
