// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The variables of a run: the port of `group_vars/all.yaml` and the `charts`
//! role. They are computed once, before any task starts, and are read-only
//! afterwards, which is what lets tasks run in parallel without coordination.

use crate::configure::ctx::template_env;
use crate::configure::facts::Facts;
use crate::configure::roles::pkg;
use anyhow::{Context, Result, bail};
use minijinja::Value;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The standard usecode checkout, relative to the home directory.
const CHECKOUT: &str = "src/github.com/evgnomon/usecode";

/// The installation profile, which switches whole roles on and off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Profile {
    /// Physical Linux desktop.
    Workstation,
    /// Explicit VM installation: headless, no desktop or host-only hardware.
    Vm,
    /// WSL2.
    Wsl,
    /// VS Code dev container or a generic container.
    #[value(name = "dev_container", alias = "dev-container")]
    DevContainer,
}

impl Profile {
    pub fn name(self) -> &'static str {
        match self {
            Profile::Workstation => "workstation",
            Profile::Vm => "vm",
            Profile::Wsl => "wsl",
            Profile::DevContainer => "dev_container",
        }
    }

    fn parse(name: &str) -> Option<Profile> {
        [
            Profile::Workstation,
            Profile::Vm,
            Profile::Wsl,
            Profile::DevContainer,
        ]
        .into_iter()
        .find(|p| p.name() == name || p.name().replace('_', "-") == name)
    }

    /// Only workstations have a display.
    pub fn desktop(self) -> bool {
        self == Profile::Workstation
    }
}

/// A git identity from the `profiles` map of the user config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GitProfile {
    pub username: Option<String>,
    pub useremail: Option<String>,
    pub fullname: Option<String>,
    pub gpg_key: Option<String>,
    pub git_sign_format: Option<String>,
    /// Sign commits and tags; anything but an explicit `true` turns it off.
    pub git_sign: Option<bool>,
    pub allowed_signers_file: Option<String>,
}

/// An entry of `git_repos`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct GitRepo {
    pub url: String,
    pub prefix: Option<String>,
    pub rewrite: Option<String>,
    pub dest: Option<String>,
    pub profile: Option<String>,
}

/// The parts of `~/src/github.com/<user>/config/config.yaml` the roles read
/// directly; templates see the whole file.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UserConfig {
    pub profiles: BTreeMap<String, GitProfile>,
    pub default_user_profile: Option<String>,
    pub git_repos: Vec<GitRepo>,
    pub user_apt_packages: Option<BTreeMap<String, Option<Vec<String>>>>,
}

pub struct Vars {
    pub facts: Facts,
    pub profile: Profile,
    pub home: PathBuf,
    pub local_bin: PathBuf,
    /// `blueprint_user`.
    pub user: String,
    /// `blueprint_home`: where downloaded packages are cached.
    pub cache_dir: PathBuf,
    /// `blueprint_config_dir`: the user's private config repository.
    pub config_dir: PathBuf,
    /// Whether the user config file exists.
    pub has_config: bool,
    pub config: UserConfig,
    /// `lib/uc/roles`, holding the files and templates.
    pub roles_dir: PathBuf,
    /// The usecode checkout.
    pub usecode_dir: PathBuf,
    /// `PATH` for every command: the tool directories first.
    pub path: String,
    /// The caller's proxy settings, re-exported through sudo.
    pub proxy: Vec<(String, String)>,
    /// Everything above, plus the user config and `-e` overrides, as the
    /// template context.
    pub template: Value,
}

pub struct LoadOptions {
    pub profile: Option<Profile>,
    pub extra: Vec<String>,
    pub roles_dir: Option<PathBuf>,
    pub config_file: Option<PathBuf>,
    /// The plan reads files and templates from the roles directory, so it
    /// must be found; `uc kube configure` reads none.
    pub need_roles: bool,
    /// A section of the user config laid over its top level before `-e`,
    /// e.g. `["kube", "cp-1"]` for what only that cluster gets.
    pub overlay: Vec<String>,
}

impl Vars {
    pub fn load(opts: LoadOptions) -> Result<Vars> {
        let facts = Facts::gather();
        let home = PathBuf::from(std::env::var("HOME").context("HOME is not set")?);
        let extra = parse_extra(&opts.extra)?;
        let extra_str = |k: &str| extra.get(k).and_then(|v| v.as_str().map(str::to_string));

        let user = extra_str("blueprint_user").unwrap_or_else(|| facts.user_id.clone());
        let config_dir = home.join("src/github.com").join(&user).join("config");
        let config_file = opts
            .config_file
            .unwrap_or_else(|| config_dir.join("config.yaml"));
        let has_config = config_file.is_file();

        let profile = match opts.profile {
            Some(p) => p,
            None => match extra_str("install_profile") {
                Some(name) => Profile::parse(&name)
                    .with_context(|| format!("unknown install_profile '{name}'"))?,
                None => detect_profile(&facts),
            },
        };

        let roles_dir = match opts.roles_dir.map_or_else(|| find_roles_dir(&home), Ok) {
            // Links point into this tree and the checkout is found from it,
            // so it must not carry `..` or a relative prefix.
            Ok(dir) => std::fs::canonicalize(&dir)
                .with_context(|| format!("roles directory {}", dir.display()))?,
            Err(_) if !opts.need_roles => home.join(CHECKOUT).join("lib/uc/roles"),
            Err(err) => return Err(err),
        };
        let usecode_dir = roles_dir
            .ancestors()
            .nth(3)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join(CHECKOUT));

        let local_bin = home.join(".local/bin");
        let cache_dir = home.join(".cache/blueprint");
        let path = std::env::join_paths(
            [
                home.join(".local/share/mise/shims"),
                home.join("go/bin"),
                home.join(".cargo/bin"),
                local_bin.clone(),
                PathBuf::from("/usr/local/bin"),
                PathBuf::from("/usr/bin"),
            ]
            .into_iter()
            .chain(
                std::env::var_os("PATH")
                    .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                    .unwrap_or_default(),
            ),
        )?
        .to_string_lossy()
        .into_owned();

        let proxy = ["http_proxy", "https_proxy", "no_proxy"]
            .iter()
            .filter_map(|k| {
                let v = std::env::var(k).ok().filter(|v| !v.is_empty())?;
                Some([(k.to_string(), v.clone()), (k.to_uppercase(), v)])
            })
            .flatten()
            .collect::<Vec<_>>();

        let mut vars = Vars {
            facts,
            profile,
            home,
            local_bin,
            user,
            cache_dir,
            config_dir,
            has_config,
            config: UserConfig::default(),
            roles_dir,
            usecode_dir,
            path,
            proxy,
            template: Value::UNDEFINED,
        };

        let mut context = vars.base_context();
        if has_config {
            let mut yaml = read_config(&config_file, &mut Vec::new())?;
            apply_overlay(&mut yaml, &opts.overlay)
                .with_context(|| format!("reading {}", config_file.display()))?;
            let rendered = render_strings(yaml, &Value::from(context.clone()))
                .with_context(|| format!("rendering {}", config_file.display()))?;
            vars.config = serde_yaml::from_value(rendered.clone())
                .with_context(|| format!("reading {}", config_file.display()))?;
            if let serde_yaml::Value::Mapping(map) = rendered {
                for (k, v) in map {
                    if let Some(k) = k.as_str() {
                        context.insert(k.to_string(), Value::from_serialize(&v));
                    }
                }
            }
        }
        for (k, v) in extra {
            context.insert(k, Value::from_serialize(&v));
        }
        vars.template = Value::from(context);
        Ok(vars)
    }

    /// The variables Ansible had before the user config was read, under
    /// their Ansible names.
    fn base_context(&self) -> BTreeMap<String, Value> {
        let f = &self.facts;
        let home = self.home.display().to_string();
        let facts = BTreeMap::from([
            ("distribution", Value::from(f.distribution.clone())),
            (
                "distribution_release",
                Value::from(f.distribution_release.clone()),
            ),
            (
                "distribution_version",
                Value::from(f.distribution_version.clone()),
            ),
            (
                "distribution_major_version",
                Value::from(f.distribution_major_version.clone()),
            ),
            ("os_family", Value::from(f.os_family.clone())),
            ("architecture", Value::from(f.architecture.clone())),
            ("kernel", Value::from(f.kernel.clone())),
            ("user_id", Value::from(f.user_id.clone())),
            ("memtotal_mb", Value::from(f.memtotal_mb)),
            ("env", Value::from_serialize(&f.env)),
        ]);
        let path = |p: &Path| Value::from(p.display().to_string());
        let proxy: BTreeMap<&str, &str> = self
            .proxy
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let entries: Vec<(&str, Value)> = vec![
            ("ansible_facts", Value::from(facts)),
            ("ansible_memtotal_mb", Value::from(f.memtotal_mb)),
            ("blueprint_user", Value::from(self.user.clone())),
            ("home", Value::from(home.clone())),
            ("target_home", Value::from(home.clone())),
            ("blueprint_user_home", Value::from(home)),
            ("local_bin_path", path(&self.local_bin)),
            ("blueprint_home", path(&self.cache_dir)),
            ("blueprint_config_dir", path(&self.config_dir)),
            ("blueprint_config_file_exists", Value::from(self.has_config)),
            ("installation_profile", Value::from(self.profile.name())),
            (
                "dev_container",
                Value::from(self.profile == Profile::DevContainer),
            ),
            ("wsl2_kernel", Value::from(f.wsl2())),
            ("python_version", Value::from("3.14.3")),
            ("go_latest_version", Value::from("1.21.6")),
            ("pinentry_bin", Value::from("/usr/bin/pinentry")),
            ("pkg_versions", pkg::versions()),
            (
                "extrepo_debian_release",
                Value::from(self.extrepo_release()),
            ),
            ("proxy_env", Value::from_serialize(&proxy)),
            ("usecode_repo_dir", path(&self.usecode_dir)),
        ];
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    }

    /// extrepo-data only publishes Debian suites; Ubuntu reads trixie's.
    pub fn extrepo_release(&self) -> String {
        if self.facts.distribution == "Ubuntu" {
            "trixie".to_string()
        } else {
            self.facts.distribution_release.clone()
        }
    }

    /// A role's directory under `lib/uc/roles`.
    pub fn role(&self, name: &str) -> PathBuf {
        self.roles_dir.join(name)
    }

    /// The value of `proxy_env.http_proxy`.
    pub fn http_proxy(&self) -> Option<&str> {
        self.env_proxy("http_proxy")
    }

    pub fn env_proxy(&self, key: &str) -> Option<&str> {
        self.proxy
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[cfg(test)]
    pub fn for_tests() -> Vars {
        let home = std::env::temp_dir();
        let mut vars = Vars {
            facts: Facts {
                distribution: "Debian".into(),
                distribution_release: "trixie".into(),
                os_family: "Debian".into(),
                architecture: "x86_64".into(),
                ..Facts::default()
            },
            profile: Profile::Workstation,
            local_bin: home.join(".local/bin"),
            cache_dir: home.join(".cache/blueprint"),
            config_dir: home.join("config"),
            home,
            user: "tester".into(),
            has_config: false,
            config: UserConfig::default(),
            roles_dir: PathBuf::from("/nonexistent/roles"),
            usecode_dir: PathBuf::from("/nonexistent"),
            path: std::env::var("PATH").unwrap_or_default(),
            proxy: Vec::new(),
            template: Value::UNDEFINED,
        };
        vars.template = Value::from(vars.base_context());
        vars
    }
}

/// `install_profile` unset: the environment decides, as in `play.sh`.
fn detect_profile(facts: &Facts) -> Profile {
    let dev_container = std::env::var("DEV_CONTAINER").is_ok_and(|v| !v.is_empty());
    if dev_container {
        Profile::DevContainer
    } else if facts.wsl2() {
        Profile::Wsl
    } else {
        Profile::Workstation
    }
}

/// The roles next to the checkout uc-configure runs from, found from the
/// working directory or the executable, else the standard checkout.
fn find_roles_dir(home: &Path) -> Result<PathBuf> {
    let from = |p: PathBuf| -> Vec<PathBuf> { p.ancestors().map(Path::to_path_buf).collect() };
    let mut bases = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        bases.extend(from(cwd));
    }
    if let Ok(exe) = std::env::current_exe() {
        bases.extend(from(exe));
    }
    bases.push(home.join(CHECKOUT));
    let found = bases
        .iter()
        .map(|b| b.join("lib/uc/roles"))
        .find(|d| d.is_dir());
    match found {
        Some(dir) => Ok(dir),
        None => bail!("cannot find lib/uc/roles; run from the usecode checkout or pass --roles"),
    }
}

/// `-e key=value`, with the value read as YAML so `-e flag=true` is a bool.
fn parse_extra(items: &[String]) -> Result<BTreeMap<String, serde_yaml::Value>> {
    let mut extra = BTreeMap::new();
    for item in items {
        let Some((k, v)) = item.split_once('=') else {
            bail!("-e expects KEY=VALUE, got '{item}'");
        };
        let value = serde_yaml::from_str(v).unwrap_or(serde_yaml::Value::String(v.to_string()));
        extra.insert(k.trim().to_string(), value);
    }
    Ok(extra)
}

/// The key listing the files a config file pulls in.
const INCLUDE: &str = "include";

/// Reads a config file with everything it includes: `include:` names files
/// or directories (their `*.yaml` and `*.yml`, in name order), relative to
/// the including file. Included files are merged in order, and the file's
/// own keys win over all of them. `seen` holds the files being read, to
/// catch an include loop.
fn read_config(path: &Path, seen: &mut Vec<PathBuf>) -> Result<serde_yaml::Value> {
    use serde_yaml::Value as Y;
    let canonical =
        std::fs::canonicalize(path).with_context(|| format!("reading {}", path.display()))?;
    if seen.contains(&canonical) {
        bail!("{} includes itself", path.display());
    }
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut own: Y =
        serde_yaml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    if own.is_null() {
        own = Y::Mapping(Default::default());
    }
    let Y::Mapping(map) = &mut own else {
        bail!("{} is not a mapping of keys", path.display());
    };
    let includes = match map.remove(INCLUDE) {
        None | Some(Y::Null) => Vec::new(),
        Some(Y::String(one)) => vec![one],
        Some(many) => serde_yaml::from_value(many)
            .with_context(|| format!("{}: include is not a list of paths", path.display()))?,
    };
    let base = path.parent().unwrap_or(Path::new("."));
    seen.push(canonical);
    let mut merged = Y::Mapping(Default::default());
    for include in includes {
        for file in include_files(&base.join(&include))
            .with_context(|| format!("{}: include {include}", path.display()))?
        {
            merge(&mut merged, read_config(&file, seen)?);
        }
    }
    seen.pop();
    merge(&mut merged, own);
    Ok(merged)
}

/// A file as itself, a directory as its YAML files in name order.
fn include_files(path: &Path) -> Result<Vec<PathBuf>> {
    if !path.is_dir() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut files = std::fs::read_dir(path)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.retain(|f| f.is_file() && f.extension().is_some_and(|e| e == "yaml" || e == "yml"));
    files.sort();
    Ok(files)
}

/// Lays `over` onto `base`: mappings merge key by key, anything else is
/// replaced.
fn merge(base: &mut serde_yaml::Value, over: serde_yaml::Value) {
    use serde_yaml::Value as Y;
    match (base, over) {
        (Y::Mapping(base), Y::Mapping(over)) => {
            for (k, v) in over {
                match base.get_mut(&k) {
                    Some(slot) => merge(slot, v),
                    None => {
                        base.insert(k, v);
                    }
                }
            }
        }
        (base, over) => *base = over,
    }
}

/// Merges the section at `path` (e.g. `kube.cp-1`) over the top level, if
/// the config has it.
fn apply_overlay(config: &mut serde_yaml::Value, path: &[String]) -> Result<()> {
    if path.is_empty() {
        return Ok(());
    }
    let mut section = &*config;
    for key in path {
        match section.get(key.as_str()) {
            Some(next) => section = next,
            None => return Ok(()),
        }
    }
    if section.is_null() {
        return Ok(());
    }
    if !section.is_mapping() {
        bail!("{} is not a mapping of keys", path.join("."));
    }
    let section = section.clone();
    merge(config, section);
    Ok(())
}

/// Renders every Jinja string in the user config, as Ansible does lazily.
fn render_strings(value: serde_yaml::Value, ctx: &Value) -> Result<serde_yaml::Value> {
    use serde_yaml::Value as Y;
    Ok(match value {
        Y::String(s) if s.contains("{{") || s.contains("{%") => {
            Y::String(template_env().render_str(&s, ctx)?)
        }
        Y::Sequence(items) => Y::Sequence(
            items
                .into_iter()
                .map(|v| render_strings(v, ctx))
                .collect::<Result<_>>()?,
        ),
        Y::Mapping(map) => Y::Mapping(
            map.into_iter()
                .map(|(k, v)| Ok((k, render_strings(v, ctx)?)))
                .collect::<Result<_>>()?,
        ),
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_config_strings_against_facts() {
        let vars = Vars::for_tests();
        let yaml: serde_yaml::Value =
            serde_yaml::from_str("prefix: \"{{ ansible_facts['distribution'] }}/src\"\nn: 3\n")
                .unwrap();
        let out = render_strings(yaml, &vars.template).unwrap();
        assert_eq!(out["prefix"].as_str(), Some("Debian/src"));
        assert_eq!(out["n"].as_i64(), Some(3));
    }

    #[test]
    fn config_includes_merge_under_the_including_file() {
        let dir = std::env::temp_dir().join(format!("uc-vars-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("conf.d")).unwrap();
        let write = |name: &str, body: &str| std::fs::write(dir.join(name), body).unwrap();
        write(
            "config.yaml",
            "include: [base.yaml, conf.d]\na: main\nkube:\n  cp-1:\n    host: main\n",
        );
        write("base.yaml", "a: base\nb: base\n");
        write(
            "conf.d/10-kube.yaml",
            "kube:\n  cp-1:\n    host: inc\n    size: 5Gi\n",
        );
        write("conf.d/notes.txt", "not: read\n");
        let mut out = read_config(&dir.join("config.yaml"), &mut Vec::new()).unwrap();
        assert_eq!(out["a"].as_str(), Some("main"));
        assert_eq!(out["b"].as_str(), Some("base"));
        assert_eq!(out["kube"]["cp-1"]["host"].as_str(), Some("main"));
        assert_eq!(out["kube"]["cp-1"]["size"].as_str(), Some("5Gi"));
        assert!(out.get("include").is_none() && out.get("not").is_none());

        apply_overlay(&mut out, &["kube".into(), "cp-1".into()]).unwrap();
        assert_eq!(out["host"].as_str(), Some("main"));
        apply_overlay(&mut out, &["kube".into(), "other".into()]).unwrap();

        write("base.yaml", "include: config.yaml\n");
        let err = read_config(&dir.join("config.yaml"), &mut Vec::new()).unwrap_err();
        assert!(format!("{err:#}").contains("includes itself"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn extra_vars_are_typed() {
        let extra = parse_extra(&["a=true".into(), "b=x y".into()]).unwrap();
        assert_eq!(extra["a"].as_bool(), Some(true));
        assert_eq!(extra["b"].as_str(), Some("x y"));
        assert!(parse_extra(&["nope".into()]).is_err());
    }

    #[test]
    fn profile_names_match_ansible() {
        assert_eq!(Profile::parse("dev_container"), Some(Profile::DevContainer));
        assert_eq!(Profile::parse("vm"), Some(Profile::Vm));
        assert_eq!(Profile::parse("nope"), None);
    }
}
