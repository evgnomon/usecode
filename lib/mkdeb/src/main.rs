// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! mkdeb - package files into a Debian (.deb) package.

mod args;
mod build;
mod config;
mod log;
mod sys;

use std::process::Command;

use args::Opts;
use build::Meta;
use config::DebJson;

/// Map `uname -m` output to a Debian architecture name.
fn debian_arch(machine: &str) -> String {
    match machine {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "armv7l" => "armhf",
        "i686" => "i386",
        m => m,
    }
    .to_string()
}

/// Lowercase and replace underscores with dashes.
fn sanitize_name(name: &str) -> String {
    name.to_lowercase().replace('_', "-")
}

fn non_empty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.is_empty())
}

/// Resolve metadata: CLI value, then .deb.json value, then default.
fn resolve(o: &Opts, j: &DebJson, source: &str) -> Meta {
    // For these, an empty CLI value counts as "not given" (as in the original).
    let weak = |cli: &Option<String>, key| non_empty(cli.clone()).or_else(|| j.get(key));
    let strong = |cli: &Option<String>, key, default: &str| {
        cli.clone()
            .or_else(|| j.get(key))
            .unwrap_or_else(|| default.to_string())
    };
    let name = weak(&o.name, "name").unwrap_or_else(|| {
        sys::logical_abs(source)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    Meta {
        name: sanitize_name(&name),
        version: weak(&o.version, "version").unwrap_or_else(|| "0.1.0".to_string()),
        arch: weak(&o.arch, "arch")
            .unwrap_or_else(|| debian_arch(&sys::capture(Command::new("uname").arg("-m")))),
        description: strong(&o.description, "description", ""),
        maintainer: strong(&o.maintainer, "maintainer", "maintainer@example.com"),
        depends: strong(&o.depends, "depends", ""),
        section: strong(&o.section, "section", "utils"),
        priority: strong(&o.priority, "priority", "optional"),
        homepage: strong(&o.homepage, "homepage", ""),
        license: strong(&o.license, "license", "HGL"),
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let o = match args::parse(&argv) {
        Ok(None) => {
            print!("{}", args::HELP);
            return;
        }
        Ok(Some(o)) => o,
        Err(msg) => log::die(&msg),
    };
    let source = o.source.clone().unwrap_or_else(|| ".".to_string());
    let output = o.output.clone().unwrap_or_else(|| "./dist".to_string());
    let prefix = o.prefix.clone().unwrap_or_else(|| "/usr".to_string());

    // Prefer .deb.jsonc when both exist.
    let jsonc = format!("{source}/.deb.jsonc");
    let j = match std::path::Path::new(&jsonc).is_file() {
        true => config::load(&jsonc),
        false => config::load(&format!("{source}/.deb.json")),
    };
    let m = resolve(&o, &j, &source);
    let root = non_empty(j.get("root"));
    let tmp_dir = non_empty(o.tmp_dir.clone()).or_else(|| j.get("tmpDir"));

    println!();
    println!("========================================");
    println!("  Debian Package Builder");
    println!("========================================");
    println!();
    log::info(&format!("Package: {}", m.name));
    log::info(&format!("Version: {}", m.version));
    log::info(&format!("Architecture: {}", m.arch));
    log::info(&format!("Description: {}", m.description));
    println!();

    log::info("Checking requirements...");
    if !sys::in_path("dpkg-deb") {
        log::die("dpkg-deb not found. Please install dpkg.");
    }

    let build_dir = build::build_dir(tmp_dir.as_deref());
    build::prepare_output(&output);
    if let Some(root) = &root {
        build::install_root(&source, root, &build_dir);
    }
    build::create_control(&build_dir, &m);
    build::create_postinst(&build_dir);
    build::create_copyright(&build_dir, &prefix, &m, &sys::logical_abs(&source));

    let deb_file = format!("{output}/{}_{}_{}.deb", m.name, m.version, m.arch);
    build::build_deb(&build_dir, &deb_file, tmp_dir.is_some());

    println!();
    log::success(&format!("Done! Install with: sudo dpkg -i {deb_file}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_mapping() {
        assert_eq!(debian_arch("x86_64"), "amd64");
        assert_eq!(debian_arch("aarch64"), "arm64");
        assert_eq!(debian_arch("riscv64"), "riscv64");
    }

    #[test]
    fn name_sanitizing() {
        assert_eq!(sanitize_name("My_App"), "my-app");
    }

    #[test]
    fn cli_overrides_json() {
        let j = DebJson::parse(r#"{"name":"j","description":"jd","license":"MIT"}"#).unwrap();
        let o = Opts {
            name: Some(String::new()),
            description: Some("cd".into()),
            arch: Some("all".into()),
            ..Opts::default()
        };
        let m = resolve(&o, &j, ".");
        assert_eq!(m.name, "j");
        assert_eq!(m.description, "cd");
        assert_eq!(m.license, "MIT");
        assert_eq!(m.maintainer, "maintainer@example.com");
    }
}
