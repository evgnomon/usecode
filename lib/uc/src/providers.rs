// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The coding agent's model providers and their keys, read from `agent.yaml`
//! in the user config (`~/.config/usecode/agent.yaml`), not from the
//! environment:
//!
//! ```yaml
//! providers:
//!   openrouter:
//!     api_key: sk-or-...
//!   claude:                       # uc agent login claude writes this one
//!     api_key: sk-ant-oat01-...
//!   copilot:                      # and uc agent login copilot this one
//!     api_key: gho_...
//!   groq:                         # any server that speaks OpenAI's API
//!     base_url: https://api.groq.com/openai/v1
//!     api_key: gsk_...
//!     models: [llama-3.3-70b-versatile]
//!   local:
//!     base_url: http://localhost:11434/v1   # no key needed
//! ```
//!
//! Each entry under `providers:` is one provider, named by its key; the
//! session starts on the first with a key, unless `UC_PROVIDER` names
//! another. `kind` says how it is spoken to:
//!
//! - `openai`: OpenAI's chat completions at `base_url` (`/chat/completions`,
//!   and `/models` for the list), the key as a bearer token. Any provider
//!   that speaks it is one entry away. A provider without a key is still on
//!   when it has a `base_url`, for servers on the machine.
//! - `openrouter`: the same, with OpenRouter's own fields (reasoning effort,
//!   usage with cost) and its model list with windows and prices.
//! - `claude`: a Claude subscription token, see [`crate::claude`].
//! - `copilot`: a GitHub token with Copilot, see [`crate::copilot`].
//!
//! The kind can be left out when the name says it: `openrouter`, `claude`,
//! `copilot`, and `openai` (OpenAI's own API); anything else is `openai`.
//! The other fields, all optional:
//!
//! - `api_key` (or `key`, `token`): the key.
//! - `base_url`: where the API is; OpenRouter's and OpenAI's are known.
//! - `models`: the models offered first in `uc agent model`.
//! - `default_model`: the one a session on it starts on, else the first of
//!   `models`, else the first listed.
//! - `list_models`: `false` offers only `models`, without asking `/models`.
//! - `headers`: more HTTP headers sent with every request.
//!
//! The rest of the agent's settings sit beside `providers:` in the same
//! file, all optional:
//!
//! ```yaml
//! provider: claude                # the one a session starts on
//! model: anthropic/claude-opus-5.5 # and its model; uc agent model sets both
//! effort: high                    # low, medium or high reasoning
//! search:                         # web_search: Parallel, else Exa, else DuckDuckGo
//!   parallel: {api_key: ...}
//!   exa: {api_key: ...}
//! fetch:                          # fetch_url renders pages in Cloudflare's browser
//!   cloudflare: {account_id: ..., api_token: ...}
//! voice:                          # Space-to-talk, through OpenRouter
//!   model: microsoft/mai-transcribe-2
//!   language: en                  # else the locale's
//!   terms: [usecode, kubectl]     # words to expect; false for none
//! ```
//!
//! `UC_PROVIDER`, `UC_MODEL` and `UC_EFFORT` still override the first three
//! for one run; keys are only ever read from the file.

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_yaml::{Mapping, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file in the user config the providers are in.
pub const FILE: &str = "agent.yaml";

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1";
const OPENAI_URL: &str = "https://api.openai.com/v1";

/// How a provider is spoken to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// OpenAI's chat completions, as OpenRouter serves it, with its extras.
    OpenRouter,
    /// OpenAI's chat completions, plain.
    OpenAi,
    /// A Claude subscription through Anthropic's Messages API.
    Claude,
    /// GitHub Copilot.
    Copilot,
}

impl Kind {
    const ALL: [Kind; 4] = [Kind::OpenRouter, Kind::OpenAi, Kind::Claude, Kind::Copilot];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::OpenRouter => "openrouter",
            Kind::OpenAi => "openai",
            Kind::Claude => "claude",
            Kind::Copilot => "copilot",
        }
    }

    fn parse(name: &str) -> Option<Kind> {
        Kind::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(name.trim()))
    }

    /// The kind a provider named `name` is when the file doesn't say.
    fn of_name(name: &str) -> Kind {
        Kind::parse(name).unwrap_or(Kind::OpenAi)
    }
}

/// One provider from the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provider {
    /// Its key under `providers:`, what `uc agent model` and `UC_PROVIDER`
    /// name it by.
    pub name: String,
    pub kind: Kind,
    /// Where an OpenAI-style API is, without the trailing `/`.
    pub base_url: String,
    /// The key, token or nothing.
    pub key: String,
    /// The models offered first.
    pub models: Vec<String>,
    pub default_model: Option<String>,
    /// Ask the provider for the rest of its models.
    pub list_models: bool,
    pub headers: Vec<(String, String)>,
}

impl Provider {
    /// A provider named `name` of `kind`, with the defaults and no key.
    pub fn new(name: &str, kind: Kind) -> Provider {
        let base_url = match kind {
            Kind::OpenRouter => OPENROUTER_URL,
            Kind::OpenAi if name.eq_ignore_ascii_case("openai") => OPENAI_URL,
            _ => "",
        };
        let headers = match kind {
            Kind::OpenRouter => vec![("X-Title".to_string(), "usecode".to_string())],
            _ => Vec::new(),
        };
        Provider {
            name: name.to_string(),
            kind,
            base_url: base_url.to_string(),
            key: String::new(),
            models: Vec::new(),
            default_model: None,
            list_models: true,
            headers,
        }
    }

    /// How the provider is shown: `OpenRouter`, `Claude`, or its name.
    pub fn label(&self) -> String {
        label(&self.name)
    }

    /// It can be talked to: it has a key, or, speaking OpenAI's API, a
    /// `base_url` (a server on the machine takes no key).
    pub fn usable(&self) -> bool {
        !self.key.is_empty() || (self.kind == Kind::OpenAi && !self.base_url.is_empty())
    }
}

/// How a provider named `name` is shown: the known ones the way their makers
/// write them, the rest as they are named in the file.
pub fn label(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "openrouter" => "OpenRouter".into(),
        "claude" => "Claude".into(),
        "copilot" => "Copilot".into(),
        "openai" => "OpenAI".into(),
        _ => name.to_string(),
    }
}

/// The agent's settings beside `providers:`; see the module's docs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub parallel_key: Option<String>,
    pub exa_key: Option<String>,
    pub cloudflare_account: Option<String>,
    pub cloudflare_token: Option<String>,
    pub voice_model: Option<String>,
    pub voice_language: Option<String>,
    /// The words to expect; `None` turns the guessing of them off.
    pub voice_terms: Option<Vec<String>>,
}

impl Default for Settings {
    /// Nothing set, and the words to expect guessed.
    fn default() -> Settings {
        Settings {
            provider: None,
            model: None,
            effort: None,
            parallel_key: None,
            exa_key: None,
            cloudflare_account: None,
            cloudflare_token: None,
            voice_model: None,
            voice_language: None,
            voice_terms: Some(Vec::new()),
        }
    }
}

/// The settings as they are written.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct File {
    /// Read by [`parse`]; here so the file's check knows it.
    #[serde(default)]
    #[allow(dead_code)]
    providers: Option<serde_yaml::Value>,
    provider: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    #[serde(default)]
    search: Search,
    #[serde(default)]
    fetch: Fetch,
    #[serde(default)]
    voice: Voice,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Search {
    parallel: Option<Keyed>,
    exa: Option<Keyed>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Keyed {
    #[serde(alias = "key", alias = "token")]
    api_key: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Fetch {
    cloudflare: Option<Cloudflare>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Cloudflare {
    account_id: Option<String>,
    #[serde(alias = "token", alias = "browser_token")]
    api_token: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Voice {
    model: Option<String>,
    language: Option<String>,
    terms: Option<Terms>,
}

/// `terms: [a, b]`, `terms: "a, b"` or `terms: false`.
#[derive(Deserialize)]
#[serde(untagged)]
enum Terms {
    On(bool),
    List(Vec<String>),
    Words(String),
}

/// A value that is something.
fn some(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The settings written in `text`.
pub fn parse_settings(text: &str) -> Result<Settings> {
    let file: File = match serde_yaml::from_str::<Option<File>>(text)? {
        Some(file) => file,
        None => return Ok(Settings::default()),
    };
    let key = |k: Option<Keyed>| some(k.and_then(|k| k.api_key));
    let cloudflare = file.fetch.cloudflare.unwrap_or_default();
    let voice_terms = match file.voice.terms {
        None | Some(Terms::On(true)) => Some(Vec::new()),
        Some(Terms::On(false)) => None,
        Some(Terms::List(list)) => Some(list),
        Some(Terms::Words(words)) => Some(
            words
                .split([',', ' ', '\n'])
                .filter(|w| !w.is_empty())
                .map(String::from)
                .collect(),
        ),
    };
    Ok(Settings {
        provider: some(file.provider),
        model: some(file.model),
        effort: some(file.effort),
        parallel_key: key(file.search.parallel),
        exa_key: key(file.search.exa),
        cloudflare_account: some(cloudflare.account_id),
        cloudflare_token: some(cloudflare.api_token),
        voice_model: some(file.voice.model),
        voice_language: some(file.voice.language),
        voice_terms,
    })
}

/// The settings in the file, read once; the defaults when there is no file
/// or it can't be read, which is said.
pub fn settings() -> &'static Settings {
    static SETTINGS: std::sync::OnceLock<Settings> = std::sync::OnceLock::new();
    SETTINGS.get_or_init(|| {
        let path = path();
        let read = match std::fs::read_to_string(&path) {
            Ok(text) => parse_settings(&text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(err) => Err(err.into()),
        };
        read.unwrap_or_else(|err| {
            eprintln!("uc: reading {}: {err:#}", path.display());
            Settings::default()
        })
    })
}

/// One entry under `providers:` as it is written.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Entry {
    kind: Option<String>,
    #[serde(alias = "key", alias = "token")]
    api_key: Option<String>,
    base_url: Option<String>,
    #[serde(default)]
    models: Vec<String>,
    default_model: Option<String>,
    list_models: Option<bool>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
}

/// The file: `~/.config/usecode/agent.yaml`, or the config repository's
/// while that isn't linked there yet (see [`crate::secret::user_config`]).
/// With neither there yet it is the first, where a login creates it.
pub fn path() -> PathBuf {
    let found = crate::secret::user_config(FILE);
    if found.exists() {
        return found;
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let config = home.join(".config/usecode");
    if !config.exists() && found.parent().is_some_and(Path::exists) {
        return found;
    }
    config.join(FILE)
}

/// The providers written in `text`, in the order written.
pub fn parse(text: &str) -> Result<Vec<Provider>> {
    let file: Value = serde_yaml::from_str(text)?;
    let providers = match &file {
        Value::Null => return Ok(Vec::new()),
        Value::Mapping(top) => match top.get("providers") {
            None | Some(Value::Null) => return Ok(Vec::new()),
            Some(Value::Mapping(providers)) => providers,
            Some(_) => bail!("providers is not a mapping of names to providers"),
        },
        _ => bail!("the file is not a mapping"),
    };
    let mut parsed = Vec::new();
    for (name, entry) in providers {
        let name = name
            .as_str()
            .ok_or_else(|| anyhow!("a provider's name is not a string: {name:?}"))?;
        let entry: Entry = match entry {
            Value::Null => Entry::default(),
            entry => {
                serde_yaml::from_value(entry.clone()).with_context(|| format!("provider {name}"))?
            }
        };
        let kind = match entry.kind.as_deref() {
            Some(kind) => Kind::parse(kind).ok_or_else(|| {
                anyhow!("provider {name}: kind {kind} is openai, openrouter, claude or copilot")
            })?,
            None => Kind::of_name(name),
        };
        let mut provider = Provider::new(name, kind);
        if let Some(url) = entry.base_url.filter(|u| !u.trim().is_empty()) {
            provider.base_url = url.trim().trim_end_matches('/').to_string();
        }
        if kind == Kind::OpenAi && provider.base_url.is_empty() {
            bail!("provider {name}: an OpenAI-style provider needs a base_url");
        }
        provider.key = entry.api_key.unwrap_or_default().trim().to_string();
        provider.models = entry.models;
        provider.default_model = entry.default_model.filter(|m| !m.trim().is_empty());
        provider.list_models = entry.list_models.unwrap_or(true);
        provider.headers.extend(entry.headers);
        parsed.push(provider);
    }
    Ok(parsed)
}

/// Every provider in the file; none when there is no file.
pub fn load() -> Result<Vec<Provider>> {
    let path = path();
    match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text).with_context(|| format!("reading {}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

/// The providers that can be talked to; a file that can't be read is said
/// and taken as none.
pub fn usable() -> Vec<Provider> {
    match load() {
        Ok(providers) => providers.into_iter().filter(Provider::usable).collect(),
        Err(err) => {
            eprintln!("uc: {err:#}");
            Vec::new()
        }
    }
}

/// The first usable provider of `kind`.
pub fn first(kind: Kind) -> Option<Provider> {
    usable().into_iter().find(|p| p.kind == kind)
}

/// Changes the file with `change`, keeping the rest of it; returns the
/// file's path. The file is the user's only.
fn update(change: impl FnOnce(&mut Mapping) -> Result<()>) -> Result<PathBuf> {
    let path = path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
    };
    let text = changed(&text, change).with_context(|| format!("updating {}", path.display()))?;
    write_private(&path, &text)?;
    Ok(path)
}

/// `text` with `change` made to its top level.
fn changed(text: &str, change: impl FnOnce(&mut Mapping) -> Result<()>) -> Result<String> {
    let mut file: Value = serde_yaml::from_str(text)?;
    if file.is_null() {
        file = Value::Mapping(Mapping::new());
    }
    change(
        file.as_mapping_mut()
            .ok_or_else(|| anyhow!("the file is not a mapping"))?,
    )?;
    Ok(serde_yaml::to_string(&file)?)
}

/// The mapping under `key` in `map`, made if it isn't there.
fn mapping<'a>(map: &'a mut Mapping, key: &str) -> Result<&'a mut Mapping> {
    let value = map
        .entry(key.into())
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if value.is_null() {
        *value = Value::Mapping(Mapping::new());
    }
    value
        .as_mapping_mut()
        .ok_or_else(|| anyhow!("{key} is not a mapping"))
}

/// Writes `key` as the key of the provider named `name`, of `kind`, into
/// the file; returns the file's path.
pub fn save_key(name: &str, kind: Kind, key: &str) -> Result<PathBuf> {
    update(|top| set_key(top, name, kind, key))
}

/// Makes `model` on the provider `provider` the one new sessions start on.
pub fn save_choice(provider: &str, model: &str) -> Result<PathBuf> {
    update(|top| {
        set_choice(top, provider, model);
        Ok(())
    })
}

fn set_choice(top: &mut Mapping, provider: &str, model: &str) {
    top.insert("provider".into(), provider.into());
    top.insert("model".into(), model.into());
}

fn set_key(top: &mut Mapping, name: &str, kind: Kind, key: &str) -> Result<()> {
    let entry =
        mapping(mapping(top, "providers")?, name).with_context(|| format!("provider {name}"))?;
    if Kind::of_name(name) != kind {
        entry.insert("kind".into(), kind.as_str().into());
    }
    for alias in ["key", "token"] {
        entry.remove(alias);
    }
    entry.insert("api_key".into(), key.into());
    Ok(())
}

/// `text` with `key` set as the key of the provider `name`.
#[cfg(test)]
fn with_key(text: &str, name: &str, kind: Kind, key: &str) -> Result<String> {
    changed(text, |top| set_key(top, name, kind, key))
}

/// Writes `text` to `path`, readable by the user only, through a file next
/// to it so a reader never sees half of it.
fn write_private(path: &Path, text: &str) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no directory", path.display()))?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = path.with_extension("yaml.tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .with_context(|| format!("writing {}", tmp.display()))?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    // Through a link, as the user config often is, the file it points at.
    let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    std::fs::rename(&tmp, &target).with_context(|| format!("writing {}", target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn providers_are_read_in_order_with_their_kinds() {
        let providers = parse(
            "providers:\n\
             \x20 openrouter:\n    api_key: sk-or\n\
             \x20 claude:\n    token: oat\n\
             \x20 copilot:\n\
             \x20 groq:\n    base_url: https://api.groq.com/openai/v1/\n    key: gsk\n\
             \x20   models: [llama]\n    list_models: false\n\
             \x20 openai:\n    api_key: sk\n\
             \x20 router2:\n    kind: openrouter\n    api_key: sk-or-2\n    headers: {A: b}\n",
        )
        .unwrap();
        let names: Vec<&str> = providers.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "openrouter",
                "claude",
                "copilot",
                "groq",
                "openai",
                "router2"
            ]
        );
        let kinds: Vec<Kind> = providers.iter().map(|p| p.kind).collect();
        assert_eq!(
            kinds,
            [
                Kind::OpenRouter,
                Kind::Claude,
                Kind::Copilot,
                Kind::OpenAi,
                Kind::OpenAi,
                Kind::OpenRouter
            ]
        );
        assert_eq!(providers[0].base_url, OPENROUTER_URL);
        assert_eq!(providers[1].key, "oat");
        assert!(!providers[2].usable(), "copilot has no token");
        let groq = &providers[3];
        assert_eq!(groq.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(
            (groq.key.as_str(), groq.models.as_slice()),
            ("gsk", &["llama".to_string()][..])
        );
        assert!(!groq.list_models);
        assert_eq!(providers[4].base_url, OPENAI_URL);
        assert!(providers[5].headers.contains(&("A".into(), "b".into())));
        assert_eq!(providers[5].label(), "router2");
        assert_eq!(providers[0].label(), "OpenRouter");
    }

    #[test]
    fn a_server_on_the_machine_needs_no_key_but_a_url() {
        let local =
            parse("providers:\n  local:\n    base_url: http://localhost:11434/v1\n").unwrap();
        assert!(local[0].usable());
        assert!(parse("providers:\n  nowhere: {api_key: x}\n").is_err());
        assert!(parse("providers:\n  x: {kind: nope}\n").is_err());
        assert!(parse("providers:\n  x: {base_url: u, typo: 1}\n").is_err());
        assert!(parse("").unwrap().is_empty());
        assert!(parse("other: 1\n").unwrap().is_empty());
    }

    #[test]
    fn a_login_sets_the_key_and_keeps_the_rest() {
        let text = "uc: 1\nproviders:\n  groq:\n    base_url: https://x/v1\n    key: old\n";
        let text = with_key(text, "groq", Kind::OpenAi, "new").unwrap();
        let text = with_key(&text, "claude", Kind::Claude, "oat").unwrap();
        let text = with_key(&text, "work", Kind::Copilot, "gho").unwrap();
        let providers = parse(&text).unwrap();
        assert_eq!(providers[0].key, "new");
        assert_eq!(providers[0].base_url, "https://x/v1");
        assert_eq!(
            (providers[1].kind, providers[1].key.as_str()),
            (Kind::Claude, "oat")
        );
        assert_eq!(
            (providers[2].kind, providers[2].key.as_str()),
            (Kind::Copilot, "gho")
        );
        assert!(text.starts_with("uc: 1\n"), "{text}");
        assert!(
            with_key("", "claude", Kind::Claude, "t")
                .unwrap()
                .contains("api_key: t")
        );
    }

    #[test]
    fn the_other_settings_sit_beside_the_providers() {
        let settings = parse_settings(
            "provider: claude\nmodel: opus\neffort: low\n\
             search:\n  parallel: {api_key: p}\n  exa: {key: e}\n\
             fetch:\n  cloudflare: {account_id: a, api_token: t}\n\
             voice:\n  model: m\n  language: nl\n  terms: [usecode, uc]\n\
             providers:\n  openrouter: {api_key: x}\n",
        )
        .unwrap();
        assert_eq!(settings.provider.as_deref(), Some("claude"));
        assert_eq!(settings.model.as_deref(), Some("opus"));
        assert_eq!(settings.effort.as_deref(), Some("low"));
        assert_eq!(settings.parallel_key.as_deref(), Some("p"));
        assert_eq!(settings.exa_key.as_deref(), Some("e"));
        assert_eq!(settings.cloudflare_account.as_deref(), Some("a"));
        assert_eq!(settings.cloudflare_token.as_deref(), Some("t"));
        assert_eq!(settings.voice_model.as_deref(), Some("m"));
        assert_eq!(settings.voice_language.as_deref(), Some("nl"));
        assert_eq!(
            settings.voice_terms,
            Some(vec!["usecode".into(), "uc".into()])
        );
        let off = parse_settings("voice: {terms: false}\n").unwrap();
        assert_eq!(off.voice_terms, None);
        let words = parse_settings("voice: {terms: \"a, b\"}\n").unwrap();
        assert_eq!(words.voice_terms, Some(vec!["a".into(), "b".into()]));
        assert_eq!(parse_settings("").unwrap(), Settings::default());
        assert!(parse_settings("typo: 1\n").is_err());
    }

    #[test]
    fn a_choice_is_saved_beside_the_providers() {
        let text = "providers:\n  claude: {api_key: t}\nmodel: old\n";
        let text = changed(text, |top| {
            set_choice(top, "claude", "anthropic/claude-opus-5.5");
            Ok(())
        })
        .unwrap();
        let settings = parse_settings(&text).unwrap();
        assert_eq!(settings.provider.as_deref(), Some("claude"));
        assert_eq!(settings.model.as_deref(), Some("anthropic/claude-opus-5.5"));
        assert_eq!(parse(&text).unwrap()[0].key, "t");
    }

    #[test]
    fn the_saved_file_is_the_users_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.yaml");
        write_private(&path, "providers: {}\n").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "providers: {}\n");
    }
}
