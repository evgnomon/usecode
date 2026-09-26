// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc vm`: the local KVM/QEMU machines of `vm`, and the same commands on
//! Hetzner Cloud and DigitalOcean with `--provider`.
//!
//! Without `--provider` (or with `--provider local`) the command line goes to
//! `vm` untouched. With a cloud provider, `create` takes the same size options
//! and picks the cheapest server type that has at least the requested vCPUs,
//! memory and disk. The server boots with the cloud-init user-data of the
//! local machines and gets an ssh_config entry, so `ssh <name>` reaches the
//! same user wherever the machine runs. The providers are driven through
//! their REST APIs, with the token `uc cloud` uses from the repository's
//! secrets.

use crate::http;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const CONFIG_PATH: &str = "/etc/vm/config.yaml";
const SSH_CONFIG_D: &str = "/etc/ssh/ssh_config.d";
/// How often and how long to wait for provider actions: five minutes.
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const POLL_TRIES: u32 = 150;

const CLOUD_HELP: &str = "\
Cloud providers:
  --provider <local|hetzner|digitalocean>
                                     Where the VM runs (default: local). With a
                                     cloud provider, 'create' picks the cheapest
                                     server type with at least --vcpus, --memory
                                     and --disk-size, and the VM is managed with
                                     the same list, info, inspect, ip, start,
                                     stop, restart and delete commands. Cloud
                                     VMs do not need sudo. The API token is
                                     HCLOUD_TOKEN or DIGITALOCEAN_ACCESS_TOKEN,
                                     else hetzner.prod or doctl.prod in the
                                     current repository's secrets.

Cloud options for 'create':
  --location <code>                  Hetzner location or DigitalOcean region
                                     (default: the cheapest one)
  --arch <x86|arm>                   CPU architecture (default: x86)
  --image <name>                     Provider image (default: debian-13)

Cloud examples:
  uc vm create uc3 --memory 4GiB --vcpus 4 --disk-size 60G --provider hetzner
  uc vm list --provider digitalocean
  uc vm delete uc3 --provider hetzner --force
";

/// The `main` of `uc-vm`.
pub fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if args.first().is_some_and(|arg| arg == "--summary") {
        println!("{}", crate::groups::VM.summary);
        return ExitCode::SUCCESS;
    }
    let (provider, args) = match split_provider(&args) {
        Ok(split) => split,
        Err(err) => {
            eprintln!("uc vm: {err:#}");
            return ExitCode::from(2);
        }
    };
    let Some(provider) = provider else {
        if matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
            // vm's own help, followed by what `uc vm` adds to it.
            let _ = Command::new("vm").arg("--help").status();
            print!("{CLOUD_HELP}");
            return ExitCode::SUCCESS;
        }
        let args: Vec<OsString> = args.into_iter().map(OsString::from).collect();
        return crate::groups::VM.run(&args);
    };
    match run(provider, &args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc vm: {err:#}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Provider {
    Hetzner,
    DigitalOcean,
}

impl Provider {
    /// `None` is the local machine.
    fn parse(name: &str) -> Result<Option<Provider>> {
        Ok(match name {
            "local" => None,
            "hetzner" | "hcloud" => Some(Provider::Hetzner),
            "digitalocean" | "do" => Some(Provider::DigitalOcean),
            other => bail!("unknown provider '{other}' (local, hetzner or digitalocean)"),
        })
    }

    fn name(self) -> &'static str {
        match self {
            Provider::Hetzner => "hetzner",
            Provider::DigitalOcean => "digitalocean",
        }
    }

    fn api_base(self) -> &'static str {
        match self {
            Provider::Hetzner => "https://api.hetzner.cloud/v1",
            Provider::DigitalOcean => "https://api.digitalocean.com/v2",
        }
    }

    /// The largest page the list endpoints serve.
    fn per_page(self) -> u32 {
        match self {
            Provider::Hetzner => 50,
            Provider::DigitalOcean => 200,
        }
    }

    fn token_env(self) -> &'static str {
        match self {
            Provider::Hetzner => "HCLOUD_TOKEN",
            Provider::DigitalOcean => "DIGITALOCEAN_ACCESS_TOKEN",
        }
    }

    /// Where the token is in the repository's secrets.
    fn secret_key(self) -> &'static str {
        match self {
            Provider::Hetzner => "hetzner.prod",
            Provider::DigitalOcean => "doctl.prod",
        }
    }

    fn default_image(self) -> &'static str {
        match self {
            Provider::Hetzner => "debian-13",
            Provider::DigitalOcean => "debian-13-x64",
        }
    }

    fn running_status(self) -> &'static str {
        match self {
            Provider::Hetzner => "running",
            Provider::DigitalOcean => "active",
        }
    }
}

/// Takes `--provider <name>` (or `--provider=<name>`) out of the command line.
fn split_provider(args: &[String]) -> Result<(Option<Provider>, Vec<String>)> {
    let mut provider = None;
    let mut rest = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if let Some(name) = arg.strip_prefix("--provider=") {
            provider = Provider::parse(name)?;
        } else if arg == "--provider" {
            let name = iter.next().context("--provider requires a value")?;
            provider = Provider::parse(name)?;
        } else {
            rest.push(arg.clone());
        }
    }
    Ok((provider, rest))
}

/// The parts of `vm`'s `/etc/vm/config.yaml` a cloud VM shares with the
/// local ones, read the way `vm` reads them.
#[derive(Debug)]
struct VmConfig {
    cloud_init_template_path: String,
    username: String,
    identity_file: String,
    /// KiB.
    default_memory: u64,
    default_vcpus: u32,
    /// Bytes.
    default_disk_size: u64,
}

impl Default for VmConfig {
    fn default() -> Self {
        VmConfig {
            cloud_init_template_path: "/usr/share/vm/images/cloud-init".to_string(),
            username: "vm".to_string(),
            identity_file: "~/.ssh/id_ed25519".to_string(),
            default_memory: 1024 * 1024,
            default_vcpus: 2,
            default_disk_size: 10 * 1024 * 1024 * 1024,
        }
    }
}

impl VmConfig {
    fn load() -> VmConfig {
        std::fs::read_to_string(CONFIG_PATH)
            .map(|contents| VmConfig::parse(&contents))
            .unwrap_or_default()
    }

    fn parse(contents: &str) -> VmConfig {
        let mut cfg = VmConfig::default();
        for line in contents.lines() {
            let Some((key, value)) = line.trim().split_once(": ") else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "cloud_init_template_path" => cfg.cloud_init_template_path = value.to_string(),
                "username" => cfg.username = value.to_string(),
                "identity_file" => cfg.identity_file = value.to_string(),
                "default_memory" => {
                    cfg.default_memory = value.parse().unwrap_or(cfg.default_memory)
                }
                "default_vcpus" => cfg.default_vcpus = value.parse().unwrap_or(cfg.default_vcpus),
                "default_disk_size" => {
                    cfg.default_disk_size = value.parse().unwrap_or(cfg.default_disk_size)
                }
                _ => {}
            }
        }
        cfg
    }

    fn user_data(&self) -> Result<PathBuf> {
        let path = Path::new(&self.cloud_init_template_path).join("cloud-init-user-data.yaml");
        if !path.is_file() {
            bail!(
                "cloud-init user-data {} not found; it sets up the '{}' user on the VM",
                path.display(),
                self.username
            );
        }
        Ok(path)
    }
}

/// What `create` asks for.
#[derive(Debug)]
struct Spec {
    name: String,
    vcpus: u32,
    memory_mib: u64,
    disk_gib: u64,
    image: Option<String>,
    location: Option<String>,
    arch: String,
    start: bool,
    wait_for_ip: bool,
}

impl Spec {
    fn parse(args: &[String], cfg: &VmConfig) -> Result<Spec> {
        let name = args
            .first()
            .filter(|name| !name.starts_with('-'))
            .context("usage: uc vm create <name> [options] --provider <provider>")?;
        let mut spec = Spec {
            name: name.clone(),
            vcpus: cfg.default_vcpus,
            memory_mib: cfg.default_memory.div_ceil(1024),
            disk_gib: cfg.default_disk_size.div_ceil(1 << 30),
            image: None,
            location: None,
            arch: "x86".to_string(),
            start: true,
            wait_for_ip: true,
        };
        let mut iter = args[1..].iter();
        while let Some(arg) = iter.next() {
            let mut value = || {
                iter.next()
                    .with_context(|| format!("{arg} requires a value"))
            };
            match arg.as_str() {
                "--memory" => spec.memory_mib = parse_memory(value()?)?.div_ceil(1024),
                "--vcpus" => spec.vcpus = value()?.parse().context("invalid --vcpus")?,
                "--disk-size" => spec.disk_gib = parse_disk_size(value()?)?.div_ceil(1 << 30),
                "--image" => spec.image = Some(value()?.clone()),
                "--location" | "--region" => spec.location = Some(value()?.clone()),
                "--arch" => spec.arch = parse_arch(value()?)?,
                "--no-start" => spec.start = false,
                "--no-wait-ip" => spec.wait_for_ip = false,
                "--machine" | "--mount" => bail!("{arg} is only available for local VMs"),
                other => bail!("unknown option: {other}"),
            }
        }
        Ok(spec)
    }
}

fn parse_arch(value: &str) -> Result<String> {
    Ok(match value {
        "x86" | "x86_64" | "amd64" => "x86",
        "arm" | "arm64" | "aarch64" => "arm",
        other => bail!("unknown architecture '{other}' (x86 or arm)"),
    }
    .to_string())
}

/// Parses a memory size into KiB; a bare number is already KiB (as in `vm`).
fn parse_memory(value: &str) -> Result<u64> {
    parse_size(value, 1 << 20, 1 << 10, 1)
}

/// Parses a disk size into bytes; a bare number is GiB (as in `vm`).
fn parse_disk_size(value: &str) -> Result<u64> {
    parse_size(value, 1 << 30, 1 << 20, 1 << 30)
}

/// `value` with a `GiB`/`G` or `MiB`/`M` suffix, or none, times its unit.
fn parse_size(value: &str, gib: u64, mib: u64, bare: u64) -> Result<u64> {
    let units = [("GiB", gib), ("MiB", mib), ("G", gib), ("M", mib)];
    let (base, unit) = units
        .iter()
        .find_map(|(suffix, unit)| value.strip_suffix(suffix).map(|base| (base, *unit)))
        .unwrap_or((value, bare));
    let base: u64 = base
        .parse()
        .with_context(|| format!("invalid size '{value}'"))?;
    Ok(base * unit)
}

/// A server type offered in one location.
#[derive(Debug, Clone, PartialEq)]
struct Offer {
    server_type: String,
    location: String,
    arch: String,
    vcpus: u32,
    memory_mib: u64,
    disk_gib: u64,
    /// Monthly, in the provider's currency.
    price: f64,
}

/// The cheapest offer that meets the spec; ties go to the smaller machine.
fn choose<'a>(offers: &'a [Offer], spec: &Spec) -> Option<&'a Offer> {
    offers
        .iter()
        .filter(|offer| {
            offer.vcpus >= spec.vcpus
                && offer.memory_mib >= spec.memory_mib
                && offer.disk_gib >= spec.disk_gib
                && offer.arch == spec.arch
                && spec
                    .location
                    .as_ref()
                    .is_none_or(|loc| *loc == offer.location)
        })
        .min_by(|a, b| {
            a.price
                .total_cmp(&b.price)
                .then(a.vcpus.cmp(&b.vcpus))
                .then(a.memory_mib.cmp(&b.memory_mib))
                .then(a.disk_gib.cmp(&b.disk_gib))
        })
}

/// A cloud server, as `list`, `info` and the power commands need it.
#[derive(Debug, Clone, PartialEq)]
struct Server {
    id: String,
    name: String,
    status: String,
    server_type: String,
    location: String,
    ipv4: Option<String>,
    vcpus: u64,
    memory_mib: u64,
    disk_gib: u64,
}

fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn number(value: &Value) -> f64 {
    match value {
        Value::String(s) => s.parse().unwrap_or(0.0),
        other => other.as_f64().unwrap_or(0.0),
    }
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

/// `hcloud server-type list -o json`: one offer per type and location, at
/// that location's price. Deprecated types and locations are left out.
fn hetzner_offers(types: &Value) -> Vec<Offer> {
    let mut offers = Vec::new();
    for ty in array(types) {
        if ty["deprecated"].as_bool() == Some(true) || !ty["deprecation"].is_null() {
            continue;
        }
        let retired: Vec<String> = array(&ty["locations"])
            .iter()
            .filter(|loc| !loc["deprecation"].is_null())
            .map(|loc| text(&loc["name"]))
            .collect();
        for price in array(&ty["prices"]) {
            let location = text(&price["location"]);
            if retired.contains(&location) {
                continue;
            }
            offers.push(Offer {
                server_type: text(&ty["name"]),
                location,
                arch: text(&ty["architecture"]),
                vcpus: number(&ty["cores"]) as u32,
                memory_mib: (number(&ty["memory"]) * 1024.0).round() as u64,
                disk_gib: number(&ty["disk"]) as u64,
                price: number(&price["price_monthly"]["net"]),
            });
        }
    }
    offers
}

/// `doctl compute size list -o json`: one offer per size and region.
fn digitalocean_offers(sizes: &Value) -> Vec<Offer> {
    let mut offers = Vec::new();
    for size in array(sizes) {
        if size["available"].as_bool() == Some(false) {
            continue;
        }
        for region in array(&size["regions"]) {
            offers.push(Offer {
                server_type: text(&size["slug"]),
                location: text(region),
                arch: "x86".to_string(),
                vcpus: number(&size["vcpus"]) as u32,
                memory_mib: number(&size["memory"]) as u64,
                disk_gib: number(&size["disk"]) as u64,
                price: number(&size["price_monthly"]),
            });
        }
    }
    offers
}

fn hetzner_server(server: &Value) -> Server {
    let location = match &server["location"]["name"] {
        Value::Null => &server["datacenter"]["location"]["name"],
        name => name,
    };
    let ty = &server["server_type"];
    Server {
        id: text(&server["id"]),
        name: text(&server["name"]),
        status: text(&server["status"]),
        server_type: text(&ty["name"]),
        location: text(location),
        ipv4: server["public_net"]["ipv4"]["ip"]
            .as_str()
            .map(str::to_string),
        vcpus: number(&ty["cores"]) as u64,
        memory_mib: (number(&ty["memory"]) * 1024.0).round() as u64,
        disk_gib: number(&ty["disk"]) as u64,
    }
}

fn digitalocean_server(droplet: &Value) -> Server {
    let ipv4 = array(&droplet["networks"]["v4"])
        .iter()
        .find(|net| net["type"] == "public")
        .and_then(|net| net["ip_address"].as_str())
        .map(str::to_string);
    Server {
        id: text(&droplet["id"]),
        name: text(&droplet["name"]),
        status: text(&droplet["status"]),
        server_type: text(&droplet["size_slug"]),
        location: text(&droplet["region"]["slug"]),
        ipv4,
        vcpus: number(&droplet["vcpus"]) as u64,
        memory_mib: number(&droplet["memory"]) as u64,
        disk_gib: number(&droplet["disk"]) as u64,
    }
}

/// A client for the provider's REST API, over [`crate::http`].
struct Api {
    provider: Provider,
    token: String,
}

impl Api {
    /// The token is `HCLOUD_TOKEN` or `DIGITALOCEAN_ACCESS_TOKEN` when set,
    /// otherwise `hetzner.prod` or `doctl.prod` in the current repository's
    /// secret store, where `uc cloud` finds it too.
    fn connect(provider: Provider) -> Result<Api> {
        let env = provider.token_env();
        if let Some(token) = std::env::var(env).ok().filter(|token| !token.is_empty()) {
            return Ok(Api { provider, token });
        }
        let repo = crate::repo::fqn();
        let secrets = crate::secret::Store::new(&crate::secret::dir(), &repo)
            .get()
            .with_context(|| format!("reading the {repo} secrets (or set {env})"))?;
        let key = provider.secret_key();
        let token = crate::secret::field(&secrets, key)
            .filter(|token| token.is_string())
            .map(crate::secret::raw)
            .with_context(|| format!("no {key} in the {repo} secrets (or set {env})"))?;
        Ok(Api { provider, token })
    }

    /// Sends one request and returns the JSON of a 2xx response (`null`
    /// when it has no body).
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = format!("{}{path}", self.provider.api_base());
        let headers = [
            format!("Authorization: Bearer {}", self.token),
            "Content-Type: application/json".to_string(),
            "User-Agent: uc-vm".to_string(),
        ];
        let body = body.map(Value::to_string);
        let response = http::request(method, &url, &headers, body.as_deref())?;
        if !response.ok() {
            bail!(
                "{} {method} {path}: HTTP {}: {}",
                self.provider.name(),
                response.status,
                api_error(&response.body)
            );
        }
        if response.body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&response.body).with_context(|| format!("{method} {path}: bad JSON"))
    }

    /// Every item under `key` across the pages of a list endpoint.
    fn list(&self, path: &str, key: &str) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        for page in 1.. {
            let per_page = self.provider.per_page();
            let body = self.call(
                "GET",
                &format!("{path}?page={page}&per_page={per_page}"),
                None,
            )?;
            items.extend(array(&body[key]).iter().cloned());
            if !has_next_page(&body) {
                break;
            }
        }
        Ok(items)
    }

    /// Waits for the action a request started to finish. Both APIs answer
    /// with `{"action": {...}}` and serve its progress at `/actions/<id>`.
    fn wait(&self, response: &Value) -> Result<()> {
        let mut action = response["action"].clone();
        for _ in 0..POLL_TRIES {
            match action["status"].as_str() {
                Some("success" | "completed") => return Ok(()),
                Some("error" | "errored") => bail!(
                    "{} action {} failed: {}",
                    self.provider.name(),
                    text(&action["id"]),
                    api_error(&action.to_string())
                ),
                _ => {}
            }
            std::thread::sleep(POLL_INTERVAL);
            let path = format!("/actions/{}", text(&action["id"]));
            action = self.call("GET", &path, None)?["action"].clone();
        }
        bail!(
            "timed out waiting for {} action {}",
            self.provider.name(),
            text(&action["id"])
        )
    }
}

/// Whether a list response has another page: Hetzner's
/// `meta.pagination.next_page`, DigitalOcean's `links.pages.next`.
fn has_next_page(body: &Value) -> bool {
    !body["meta"]["pagination"]["next_page"].is_null() || body["links"]["pages"]["next"].is_string()
}

/// The message of an API error: Hetzner's `error.message`, DigitalOcean's
/// `message`, or the body itself.
fn api_error(body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap_or_default();
    value["error"]["message"]
        .as_str()
        .or(value["message"].as_str())
        .map_or_else(|| body.trim().to_string(), str::to_string)
}

fn offers(api: &Api) -> Result<Vec<Offer>> {
    Ok(match api.provider {
        Provider::Hetzner => {
            hetzner_offers(&Value::Array(api.list("/server_types", "server_types")?))
        }
        Provider::DigitalOcean => digitalocean_offers(&Value::Array(api.list("/sizes", "sizes")?)),
    })
}

fn servers(api: &Api) -> Result<Vec<Server>> {
    Ok(match api.provider {
        Provider::Hetzner => api
            .list("/servers", "servers")?
            .iter()
            .map(hetzner_server)
            .collect(),
        Provider::DigitalOcean => api
            .list("/droplets", "droplets")?
            .iter()
            .map(digitalocean_server)
            .collect(),
    })
}

fn find(api: &Api, name: &str) -> Result<Server> {
    servers(api)?
        .into_iter()
        .find(|server| server.name == name)
        .with_context(|| format!("VM '{name}' not found on {}", api.provider.name()))
}

fn run(provider: Provider, args: &[String]) -> Result<()> {
    let cfg = VmConfig::load();
    let Some(command) = args.first().map(String::as_str) else {
        bail!(
            "usage: uc vm <command> [options] --provider {}",
            provider.name()
        );
    };
    let rest = &args[1..];
    let name = || -> Result<&str> {
        rest.first()
            .map(String::as_str)
            .filter(|name| !name.starts_with('-'))
            .with_context(|| {
                format!(
                    "usage: uc vm {command} <name> --provider {}",
                    provider.name()
                )
            })
    };
    let force = || -> Result<bool> {
        match &rest[1..] {
            [] => Ok(false),
            [flag] if flag == "--force" => Ok(true),
            [other, ..] => bail!("unknown option: {other}"),
        }
    };
    match command {
        "-h" | "--help" => {
            print!("{CLOUD_HELP}");
            return Ok(());
        }
        "snapshot" | "fork" | "mount" | "config" => {
            bail!("'{command}' is only available for local VMs")
        }
        _ => {}
    }
    let api = &Api::connect(provider)?;
    match command {
        "create" => create(api, &cfg, &Spec::parse(rest, &cfg)?),
        "list" => list(api, rest.iter().any(|arg| arg == "-a" || arg == "--all")),
        "info" | "inspect" => info(&find(api, name()?)?),
        "ip" => {
            let server = find(api, name()?)?;
            let ip = server.ipv4.context("the VM has no public IPv4 address")?;
            println!("{ip}");
            Ok(())
        }
        "start" => power(api, &find(api, name()?)?, Power::Start),
        "stop" => {
            let server = find(api, name()?)?;
            power(
                api,
                &server,
                if force()? {
                    Power::Off
                } else {
                    Power::Shutdown
                },
            )
        }
        "restart" => {
            let server = find(api, name()?)?;
            power(
                api,
                &server,
                if force()? {
                    Power::Reset
                } else {
                    Power::Reboot
                },
            )
        }
        "delete" => delete(api, &find(api, name()?)?, force()?),
        other => bail!("unknown command '{other}'; see 'uc vm --help'"),
    }
}

fn create(api: &Api, cfg: &VmConfig, spec: &Spec) -> Result<()> {
    let provider = api.provider;
    if provider == Provider::DigitalOcean && !spec.start {
        bail!("DigitalOcean droplets always start when created; --no-start is not supported");
    }
    let user_data = std::fs::read_to_string(cfg.user_data()?)?;
    if servers(api)?.iter().any(|server| server.name == spec.name) {
        bail!("VM '{}' already exists on {}", spec.name, provider.name());
    }
    let offers = offers(api)?;
    let offer = choose(&offers, spec).with_context(|| {
        format!(
            "no {} {} server type has {} vCPUs, {} MiB memory and {} GiB disk{}",
            provider.name(),
            spec.arch,
            spec.vcpus,
            spec.memory_mib,
            spec.disk_gib,
            spec.location
                .as_ref()
                .map(|loc| format!(" in {loc}"))
                .unwrap_or_default()
        )
    })?;
    println!(
        "Creating '{}' on {} as {} ({} vCPUs, {} MiB, {} GiB) in {} at {:.2}/month",
        spec.name,
        provider.name(),
        offer.server_type,
        offer.vcpus,
        offer.memory_mib,
        offer.disk_gib,
        offer.location,
        offer.price
    );
    let image = spec.image.as_deref().unwrap_or(provider.default_image());
    let server = match provider {
        Provider::Hetzner => {
            let body = json!({
                "name": spec.name,
                "server_type": offer.server_type,
                "image": image,
                "location": offer.location,
                "user_data": user_data,
                "start_after_create": spec.start,
            });
            let response = api.call("POST", "/servers", Some(&body))?;
            if spec.start && spec.wait_for_ip {
                api.wait(&response)?;
            }
            hetzner_server(&response["server"])
        }
        Provider::DigitalOcean => {
            let body = json!({
                "name": spec.name,
                "size": offer.server_type,
                "image": image,
                "region": offer.location,
                "user_data": user_data,
            });
            let droplet =
                digitalocean_server(&api.call("POST", "/droplets", Some(&body))?["droplet"]);
            if spec.wait_for_ip {
                wait_for_droplet(api, &droplet.id)?
            } else {
                droplet
            }
        }
    };
    if !spec.start {
        println!("VM '{}' created but not started", spec.name);
        return Ok(());
    }
    println!("VM '{}' created and started", spec.name);
    if !spec.wait_for_ip {
        return Ok(());
    }
    match &server.ipv4 {
        Some(ip) => {
            println!("VM IP address: {ip}");
            register_host(cfg, &spec.name, ip);
        }
        None => eprintln!("uc vm: warning: the VM has no public IPv4 address yet"),
    }
    Ok(())
}

/// Polls a new droplet until it is active and has its public address.
fn wait_for_droplet(api: &Api, id: &str) -> Result<Server> {
    for _ in 0..POLL_TRIES {
        let droplet =
            digitalocean_server(&api.call("GET", &format!("/droplets/{id}"), None)?["droplet"]);
        if droplet.status == "active" && droplet.ipv4.is_some() {
            return Ok(droplet);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    bail!("timed out waiting for droplet {id} to become active")
}

fn list(api: &Api, all: bool) -> Result<()> {
    let servers: Vec<Server> = servers(api)?
        .into_iter()
        .filter(|server| all || server.status == api.provider.running_status())
        .collect();
    println!(
        "{:<24} {:<10} {:<14} {:<8} IPV4",
        "NAME", "STATUS", "TYPE", "LOCATION"
    );
    for s in servers {
        println!(
            "{:<24} {:<10} {:<14} {:<8} {}",
            s.name,
            s.status,
            s.server_type,
            s.location,
            s.ipv4.as_deref().unwrap_or("-")
        );
    }
    Ok(())
}

fn info(server: &Server) -> Result<()> {
    println!("Name:      {}", server.name);
    println!("ID:        {}", server.id);
    println!("Status:    {}", server.status);
    println!("Type:      {}", server.server_type);
    println!("Location:  {}", server.location);
    println!("vCPUs:     {}", server.vcpus);
    println!("Memory:    {} MiB", server.memory_mib);
    println!("Disk:      {} GiB", server.disk_gib);
    println!("IPv4:      {}", server.ipv4.as_deref().unwrap_or("-"));
    Ok(())
}

#[derive(Clone, Copy)]
enum Power {
    Start,
    Shutdown,
    Off,
    Reboot,
    Reset,
}

fn power(api: &Api, server: &Server, action: Power) -> Result<()> {
    let id = &server.id;
    let response = match api.provider {
        Provider::Hetzner => {
            let verb = match action {
                Power::Start => "poweron",
                Power::Shutdown => "shutdown",
                Power::Off => "poweroff",
                Power::Reboot => "reboot",
                Power::Reset => "reset",
            };
            api.call("POST", &format!("/servers/{id}/actions/{verb}"), None)?
        }
        Provider::DigitalOcean => {
            let verb = match action {
                Power::Start => "power_on",
                Power::Shutdown => "shutdown",
                Power::Off => "power_off",
                Power::Reboot => "reboot",
                Power::Reset => "power_cycle",
            };
            let body = json!({ "type": verb });
            api.call("POST", &format!("/droplets/{id}/actions"), Some(&body))?
        }
    };
    api.wait(&response)?;
    println!("VM '{}': done", server.name);
    Ok(())
}

fn delete(api: &Api, server: &Server, force: bool) -> Result<()> {
    if server.status == api.provider.running_status() && !force {
        bail!(
            "VM '{}' is running; stop it first or use --force",
            server.name
        );
    }
    let id = &server.id;
    match api.provider {
        Provider::Hetzner => api.wait(&api.call("DELETE", &format!("/servers/{id}"), None)?)?,
        Provider::DigitalOcean => drop(api.call("DELETE", &format!("/droplets/{id}"), None)?),
    }
    for path in ssh_config_paths(&server.name) {
        match std::fs::remove_file(&path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
                eprintln!("uc vm: warning: could not remove {}: {err}", path.display())
            }
            _ => {}
        }
    }
    println!("VM '{}' deleted", server.name);
    Ok(())
}

/// Where a VM's ssh_config entry may live: the system directory `vm` uses,
/// then `~/.ssh/config.d` for when uc runs without root.
fn ssh_config_paths(host: &str) -> Vec<PathBuf> {
    let file = format!("{host}.conf");
    let mut paths = vec![Path::new(SSH_CONFIG_D).join(&file)];
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(Path::new(&home).join(".ssh/config.d").join(&file));
    }
    paths
}

/// Writes the VM's ssh_config entry, like `vm` does; failures are non-fatal.
fn register_host(cfg: &VmConfig, host: &str, ip: &str) {
    let entry = format!(
        "Host {host}\n    HostName {ip}\n    User {}\n    Port 22\n    IdentityFile {}\n",
        cfg.username, cfg.identity_file
    );
    for path in ssh_config_paths(host) {
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, &entry));
        if written.is_ok() {
            println!("SSH config written to {}", path.display());
            return;
        }
    }
    eprintln!("uc vm: warning: could not write an ssh_config entry for '{host}'");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    fn offer(ty: &str, loc: &str, vcpus: u32, memory_mib: u64, disk_gib: u64, price: f64) -> Offer {
        Offer {
            server_type: ty.to_string(),
            location: loc.to_string(),
            arch: "x86".to_string(),
            vcpus,
            memory_mib,
            disk_gib,
            price,
        }
    }

    #[test]
    fn provider_is_taken_out_of_the_command_line() {
        let (provider, rest) = split_provider(&strings(&[
            "create",
            "uc3",
            "--provider",
            "hetzner",
            "--vcpus",
            "4",
        ]))
        .unwrap();
        assert_eq!(provider, Some(Provider::Hetzner));
        assert_eq!(rest, strings(&["create", "uc3", "--vcpus", "4"]));
        let (provider, rest) = split_provider(&strings(&["list", "--provider=do"])).unwrap();
        assert_eq!(provider, Some(Provider::DigitalOcean));
        assert_eq!(rest, strings(&["list"]));
        let (provider, _) = split_provider(&strings(&["list", "--provider", "local"])).unwrap();
        assert_eq!(provider, None);
        assert!(split_provider(&strings(&["list", "--provider", "aws"])).is_err());
        assert!(split_provider(&strings(&["list", "--provider"])).is_err());
    }

    #[test]
    fn sizes_parse_like_vm() {
        assert_eq!(parse_memory("4GiB").unwrap(), 4 << 20);
        assert_eq!(parse_memory("512M").unwrap(), 512 << 10);
        assert_eq!(parse_memory("4096").unwrap(), 4096);
        assert_eq!(parse_disk_size("60G").unwrap(), 60 << 30);
        assert_eq!(parse_disk_size("512MiB").unwrap(), 512 << 20);
        assert_eq!(parse_disk_size("10").unwrap(), 10 << 30);
        assert!(parse_disk_size("big").is_err());
    }

    #[test]
    fn spec_takes_the_vm_options_and_config_defaults() {
        let cfg = VmConfig::parse("default_vcpus: 3\nusername: hamed\n");
        let spec = Spec::parse(
            &strings(&[
                "uc3",
                "--memory",
                "4GiB",
                "--disk-size",
                "60G",
                "--location",
                "hel1",
            ]),
            &cfg,
        )
        .unwrap();
        assert_eq!(spec.name, "uc3");
        assert_eq!(spec.vcpus, 3);
        assert_eq!(spec.memory_mib, 4096);
        assert_eq!(spec.disk_gib, 60);
        assert_eq!(spec.location.as_deref(), Some("hel1"));
        assert_eq!(cfg.username, "hamed");
        assert!(Spec::parse(&strings(&["uc3", "--mount", "/src"]), &cfg).is_err());
        assert!(Spec::parse(&strings(&["--vcpus", "2"]), &cfg).is_err());
    }

    #[test]
    fn chooses_the_cheapest_type_that_is_big_enough() {
        let offers = [
            offer("small", "fsn1", 2, 4096, 40, 4.0),
            offer("big", "fsn1", 4, 8192, 80, 9.0),
            offer("big", "ash", 4, 8192, 80, 12.0),
            offer("bigger", "fsn1", 8, 16384, 160, 18.0),
        ];
        let cfg = VmConfig::default();
        let spec = Spec::parse(
            &strings(&[
                "uc3",
                "--memory",
                "4GiB",
                "--vcpus",
                "4",
                "--disk-size",
                "60G",
            ]),
            &cfg,
        )
        .unwrap();
        let chosen = choose(&offers, &spec).unwrap();
        assert_eq!(
            (chosen.server_type.as_str(), chosen.location.as_str()),
            ("big", "fsn1")
        );

        let in_ash = Spec {
            location: Some("ash".to_string()),
            ..spec
        };
        assert_eq!(choose(&offers, &in_ash).unwrap().location, "ash");
        let arm = Spec {
            arch: "arm".to_string(),
            ..in_ash
        };
        assert_eq!(choose(&offers, &arm), None);
    }

    #[test]
    fn hetzner_types_become_offers_per_location() {
        let types = json!([
            {"name": "cx33", "cores": 4, "memory": 8.0, "disk": 80, "architecture": "x86",
             "deprecation": null,
             "locations": [{"name": "fsn1", "deprecation": null},
                           {"name": "hel1", "deprecation": {"announced": "x"}}],
             "prices": [{"location": "fsn1", "price_monthly": {"net": "5.4900000000"}},
                        {"location": "hel1", "price_monthly": {"net": "5.4900000000"}}]},
            {"name": "cx11", "cores": 1, "memory": 2.0, "disk": 20, "architecture": "x86",
             "deprecated": true,
             "prices": [{"location": "fsn1", "price_monthly": {"net": "3.0"}}]}
        ]);
        assert_eq!(
            hetzner_offers(&types),
            [Offer {
                price: 5.49,
                ..offer("cx33", "fsn1", 4, 8192, 80, 0.0)
            }]
        );
    }

    #[test]
    fn digitalocean_sizes_become_offers_per_region() {
        let sizes = json!([
            {"slug": "s-4vcpu-8gb", "vcpus": 4, "memory": 8192, "disk": 160,
             "price_monthly": 48, "available": true, "regions": ["ams3", "fra1"]},
            {"slug": "gone", "vcpus": 4, "memory": 8192, "disk": 160,
             "price_monthly": 1, "available": false, "regions": ["fra1"]}
        ]);
        let offers = digitalocean_offers(&sizes);
        assert_eq!(offers.len(), 2);
        assert_eq!(offers[1], offer("s-4vcpu-8gb", "fra1", 4, 8192, 160, 48.0));
    }

    #[test]
    fn servers_map_from_the_api_output() {
        let server = hetzner_server(&json!({
            "id": 42, "name": "uc3", "status": "running",
            "server_type": {"name": "cx33", "cores": 4, "memory": 8.0, "disk": 80},
            "datacenter": {"location": {"name": "fsn1"}},
            "public_net": {"ipv4": {"ip": "1.2.3.4"}}
        }));
        assert_eq!(server.id, "42");
        assert_eq!(server.location, "fsn1");
        assert_eq!(server.memory_mib, 8192);
        assert_eq!(server.ipv4.as_deref(), Some("1.2.3.4"));

        let droplet = digitalocean_server(&json!({
            "id": 7, "name": "uc3", "status": "active", "size_slug": "s-4vcpu-8gb",
            "vcpus": 4, "memory": 8192, "disk": 160, "region": {"slug": "fra1"},
            "networks": {"v4": [{"ip_address": "10.0.0.2", "type": "private"},
                                {"ip_address": "5.6.7.8", "type": "public"}]}
        }));
        assert_eq!(droplet.location, "fra1");
        assert_eq!(droplet.ipv4.as_deref(), Some("5.6.7.8"));
    }

    #[test]
    fn pages_and_errors_read_both_apis() {
        assert!(has_next_page(
            &json!({"meta": {"pagination": {"next_page": 2}}})
        ));
        assert!(!has_next_page(
            &json!({"meta": {"pagination": {"next_page": null}}})
        ));
        assert!(has_next_page(
            &json!({"links": {"pages": {"next": "https://x?page=2"}}})
        ));
        assert!(!has_next_page(&json!({"links": {}})));
        assert_eq!(
            api_error(r#"{"error": {"code": "unauthorized", "message": "invalid token"}}"#),
            "invalid token"
        );
        assert_eq!(
            api_error(r#"{"id": "not_found", "message": "gone"}"#),
            "gone"
        );
        assert_eq!(api_error("Bad Gateway\n"), "Bad Gateway");
    }
}
