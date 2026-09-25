// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Assembling the package tree and invoking dpkg-deb.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::log;
use crate::sys;

/// Fully resolved package metadata.
#[derive(Debug)]
pub struct Meta {
    pub name: String,
    pub version: String,
    pub description: String,
    pub maintainer: String,
    pub arch: String,
    pub depends: String,
    pub section: String,
    pub priority: String,
    pub homepage: String,
    pub license: String,
}

const POSTINST: &str = "#!/bin/sh
set -e

# Update shared library cache if needed
if [ -d /usr/lib ] && command -v ldconfig >/dev/null 2>&1; then
    ldconfig
fi

exit 0
";

fn fail(what: &str, path: &str, e: std::io::Error) -> ! {
    log::die(&format!("{what} {path}: {e}"))
}

fn write(path: &str, content: impl AsRef<[u8]>) {
    fs::write(path, content).unwrap_or_else(|e| fail("Cannot write", path, e));
}

fn mkdir_p(path: &str) {
    fs::create_dir_all(path).unwrap_or_else(|e| fail("Cannot create", path, e));
}

/// Create the build directory: `tmp_dir` (wiped first) or a fresh `mktemp -d`.
pub fn build_dir(tmp_dir: Option<&str>) -> String {
    let Some(dir) = tmp_dir else {
        return sys::capture(Command::new("mktemp").arg("-d"));
    };
    let res = match fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(dir),
        Ok(_) => fs::remove_file(dir),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    };
    res.unwrap_or_else(|e| fail("Cannot remove", dir, e));
    mkdir_p(dir);
    dir.to_string()
}

/// `rm $OUTPUT_DIR/*.deb 2>/dev/null || true; mkdir -p "$OUTPUT_DIR"`
pub fn prepare_output(output: &str) {
    if let Ok(entries) = fs::read_dir(output) {
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".deb") && !name.starts_with('.') {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    mkdir_p(output);
}

/// Copy the contents of `$SOURCE_DIR/$root` into the build root.
pub fn install_root(source: &str, root: &str, build: &str) {
    let root_dir = format!("{source}/{root}");
    if !Path::new(&root_dir).is_dir() {
        log::die(&format!("Root directory not found: {root_dir}"));
    }
    log::info(&format!("Installing root hierarchy from {root_dir}/"));
    sys::run(
        Command::new("cp")
            .arg("-rL")
            .arg("--preserve=mode")
            .arg(format!("{root_dir}/."))
            .arg(format!("{build}/")),
    );
}

pub fn control_text(m: &Meta, installed_size: &str) -> String {
    let mut s = format!(
        "Package: {}\nVersion: {}\nSection: {}\nPriority: {}\nArchitecture: {}\n\
         Maintainer: {}\nInstalled-Size: {installed_size}\nDescription: {}\n",
        m.name, m.version, m.section, m.priority, m.arch, m.maintainer, m.description
    );
    if !m.depends.is_empty() {
        s.push_str(&format!("Depends: {}\n", m.depends));
    }
    if !m.homepage.is_empty() {
        s.push_str(&format!("Homepage: {}\n", m.homepage));
    }
    s
}

pub fn create_control(build: &str, m: &Meta) {
    let debian = format!("{build}/DEBIAN");
    mkdir_p(&debian);
    let du = sys::capture(Command::new("du").arg("-sk").arg(build));
    let size = du.split('\t').next().unwrap_or("");
    log::info("Creating DEBIAN/control...");
    write(&format!("{debian}/control"), control_text(m, size));
}

/// Install the default postinst unless the package root already ships one.
pub fn create_postinst(build: &str) {
    let path = format!("{build}/DEBIAN/postinst");
    match Path::new(&path).is_file() {
        true => log::info("Keeping DEBIAN/postinst from package root"),
        false => write(&path, POSTINST),
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| fail("Cannot chmod", &path, e));
}

/// Walk up from `start` looking for COPYING (preferred) or LICENSE.
pub fn find_license(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|d| {
        ["COPYING", "LICENSE"]
            .iter()
            .map(|f| d.join(f))
            .find(|p| p.is_file())
    })
}

/// Debian copyright formatting of the license body: the equivalent of
/// `echo "$(cat file)" | sed 's/^$/ ./; s/^/ /'`, so blank lines become
/// "  ." and all other lines get a single leading space.
pub fn format_license(text: &[u8]) -> Vec<u8> {
    let text: Vec<u8> = text.iter().copied().filter(|&b| b != 0).collect();
    let end = text.iter().rposition(|&b| b != b'\n').map_or(0, |i| i + 1);
    if end == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, line) in text[..end].split(|&b| b == b'\n').enumerate() {
        if i > 0 {
            out.push(b'\n');
        }
        match line.is_empty() {
            true => out.extend_from_slice(b"  ."),
            false => {
                out.push(b' ');
                out.extend_from_slice(line);
            }
        }
    }
    out
}

pub fn create_copyright(build: &str, prefix: &str, m: &Meta, source_abs: &Path) {
    let doc_dir = format!("{build}{prefix}/share/doc/{}", m.name);
    mkdir_p(&doc_dir);

    let mut formatted = Vec::new();
    match find_license(source_abs) {
        Some(file) => {
            log::info(&format!("Using license file: {}", file.display()));
            let text =
                fs::read(&file).unwrap_or_else(|e| fail("Cannot read", &file.to_string_lossy(), e));
            formatted = format_license(&text);
        }
        None => println!("No COPYING or LICENSE file found"),
    }

    let year = sys::capture(Command::new("date").arg("+%Y"));
    let mut content = format!(
        "Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n\
         Upstream-Name: {}\nFiles: *\nCopyright: {year} {}\nLicense: {}\n\nLicense: {}\n",
        m.name, m.maintainer, m.license, m.license
    )
    .into_bytes();
    content.extend_from_slice(&formatted);
    content.push(b'\n');
    write(&format!("{doc_dir}/copyright"), content);
}

/// Make every directory 0755 and strip setuid/setgid/sticky bits from
/// every other entry, keeping the remaining mode bits.
pub fn fix_permissions(path: &Path) {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    let set = |mode| {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .unwrap_or_else(|e| fail("Cannot chmod", &path.to_string_lossy(), e))
    };
    if meta.is_dir() {
        set(0o755);
        let entries =
            fs::read_dir(path).unwrap_or_else(|e| fail("Cannot read", &path.to_string_lossy(), e));
        for e in entries.flatten() {
            fix_permissions(&e.path());
        }
    } else if !meta.file_type().is_symlink() && meta.permissions().mode() & 0o7000 != 0 {
        set(meta.permissions().mode() & 0o777);
    }
}

pub fn build_deb(build: &str, deb_file: &str, tmp_dir_set: bool) {
    log::info("Building .deb package...");
    fix_permissions(Path::new(build));

    let dpkg_tmp = match tmp_dir_set {
        true => sys::dirname(
            &Path::new(build)
                .canonicalize()
                .unwrap_or_else(|e| fail("Cannot resolve", build, e)),
        ),
        false => sys::dirname(Path::new(build)),
    };
    let dpkg =
        |args: &[&str]| sys::run(Command::new("dpkg-deb").args(args).env("TMPDIR", &dpkg_tmp));

    dpkg(&["--build", "--root-owner-group", build, deb_file]);
    log::success(&format!("Package created: {deb_file}"));

    println!();
    log::info("Package information:");
    dpkg(&["--info", deb_file]);

    println!();
    log::info("Package contents:");
    dpkg(&["--contents", deb_file]);

    // `tree` is informational only; skip it when it is not installed.
    if let Ok(status) = Command::new("tree").arg(build).status()
        && !status.success()
    {
        std::process::exit(status.code().unwrap_or(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> Meta {
        Meta {
            name: "app".into(),
            version: "1.0".into(),
            description: "An app".into(),
            maintainer: "m@x".into(),
            arch: "amd64".into(),
            depends: String::new(),
            section: "utils".into(),
            priority: "optional".into(),
            homepage: "https://h".into(),
            license: "HGL".into(),
        }
    }

    #[test]
    fn control() {
        assert_eq!(
            control_text(&meta(), "12"),
            "Package: app\nVersion: 1.0\nSection: utils\nPriority: optional\n\
             Architecture: amd64\nMaintainer: m@x\nInstalled-Size: 12\n\
             Description: An app\nHomepage: https://h\n"
        );
    }

    #[test]
    fn license_formatting() {
        assert_eq!(format_license(b"a\n\nb\n\n\n"), b" a\n  .\n b".to_vec());
        assert_eq!(format_license(b"\n\n"), Vec::<u8>::new());
        assert_eq!(format_license(b"\nx"), b"  .\n x".to_vec());
    }

    #[test]
    fn postinst_kept_or_defaulted() {
        let dir = std::env::temp_dir().join(format!("mkdeb-test-{}", std::process::id()));
        let build = dir.to_string_lossy().to_string();
        fs::create_dir_all(dir.join("DEBIAN")).unwrap();

        create_postinst(&build);
        let path = dir.join("DEBIAN/postinst");
        assert_eq!(fs::read_to_string(&path).unwrap(), POSTINST);

        fs::write(&path, "#!/bin/sh\nsystemctl start x\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        create_postinst(&build);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "#!/bin/sh\nsystemctl start x\n"
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );

        fs::remove_dir_all(&dir).unwrap();
    }
}
