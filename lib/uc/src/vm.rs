// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc vm`: the local KVM/QEMU machines of `vm`, and the same commands on
//! Hetzner Cloud, DigitalOcean and OVHcloud Public Cloud with `--provider`.
//!
//! Without `--provider` (or with `--provider local`) the command line goes to
//! `vm` untouched. With a cloud provider, `create` takes the same size options
//! and picks the cheapest server type that has at least the requested vCPUs,
//! memory and disk. The server boots with the cloud-init user-data of the
//! local machines and gets an ssh_config entry, so `ssh <name>` reaches the
//! same user wherever the machine runs. The providers are driven through
//! their REST APIs, with the credentials `uc cloud` uses from the
//! repository's secrets.

use crate::http;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

const CONFIG_PATH: &str = "/etc/vm/config.yaml";
const SSH_CONFIG_D: &str = "/etc/ssh/ssh_config.d";
/// How often and how long to wait for provider actions: five minutes.
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const POLL_TRIES: u32 = 150;
/// OVHcloud prices by the hour; a month of it, to compare with the others.
const HOURS_PER_MONTH: f64 = 730.0;

const CLOUD_HELP: &str = "\
Cloud providers:
  --provider <local|hetzner|digitalocean|ovh>
                                     Where the VM runs (default: local). With a
                                     cloud provider, 'create' picks the cheapest
                                     server type with at least --vcpus, --memory
                                     and --disk-size, and the VM is managed with
                                     the same list, info, inspect, ip, start,
                                     stop, restart and delete commands. Cloud
                                     VMs do not need sudo. The API token is
                                     HCLOUD_TOKEN or DIGITALOCEAN_ACCESS_TOKEN,
                                     else hetzner.prod or doctl.prod in the
                                     current repository's secrets. OVHcloud
                                     takes OVH_APPLICATION_KEY,
                                     OVH_APPLICATION_SECRET, OVH_CONSUMER_KEY,
                                     OVH_CLOUD_PROJECT_SERVICE and optionally
                                     OVH_ENDPOINT (ovh-eu, ovh-ca or ovh-us),
                                     else application_key, application_secret,
                                     consumer_key, project and endpoint under
                                     ovh.prod in the secrets.

Cloud options for 'create':
  --location <code>                  Hetzner location, DigitalOcean region or
                                     OVHcloud region such as GRA11
                                     (default: the cheapest one)
  --arch <x86|arm>                   CPU architecture (default: x86)
  --image <name>                     Provider image (default: debian-13, or
                                     'Debian 13' on OVHcloud)

Cloud examples:
  uc vm create uc3 --memory 4GiB --vcpus 4 --disk-size 60G --provider hetzner
  uc vm list --provider digitalocean
  uc vm create uc4 --vcpus 2 --memory 8GiB --provider ovh --location GRA11
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
    Ovh,
}

impl Provider {
    /// `None` is the local machine.
    fn parse(name: &str) -> Result<Option<Provider>> {
        Ok(match name {
            "local" => None,
            "hetzner" | "hcloud" => Some(Provider::Hetzner),
            "digitalocean" | "do" => Some(Provider::DigitalOcean),
            "ovh" | "ovhcloud" => Some(Provider::Ovh),
            other => bail!("unknown provider '{other}' (local, hetzner, digitalocean or ovh)"),
        })
    }

    fn name(self) -> &'static str {
        match self {
            Provider::Hetzner => "hetzner",
            Provider::DigitalOcean => "digitalocean",
            Provider::Ovh => "ovh",
        }
    }

    /// The largest page the list endpoints serve; OVHcloud's lists are not
    /// paged.
    fn per_page(self) -> u32 {
        match self {
            Provider::Hetzner => 50,
            Provider::DigitalOcean | Provider::Ovh => 200,
        }
    }

    fn default_image(self) -> &'static str {
        match self {
            Provider::Hetzner => "debian-13",
            Provider::DigitalOcean => "debian-13-x64",
            Provider::Ovh => "Debian 13",
        }
    }

    fn running_status(self) -> &'static str {
        match self {
            Provider::Hetzner => "running",
            Provider::DigitalOcean => "active",
            Provider::Ovh => "ACTIVE",
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

/// A flavor or image of OVHcloud that is missing, when there is none.
static NULL: Value = Value::Null;

/// `GET /cloud/project/<project>/flavor`: OVHcloud lists each flavor once per
/// region already. Flavors carry no price, so it comes from the hourly plan
/// of the public catalog (in 10^-8 of its currency), a month being
/// [`HOURS_PER_MONTH`]; so does the memory, which the catalog gives in whole
/// GiB. Windows flavors and flavors the catalog does not price are left out.
fn ovh_offers(flavors: &Value, catalog: &Value) -> Vec<Offer> {
    let plans: HashMap<String, &Value> = array(&catalog["addons"])
        .iter()
        .map(|plan| (text(&plan["planCode"]), plan))
        .collect();
    let mut offers = Vec::new();
    for flavor in array(flavors) {
        if flavor["available"].as_bool() == Some(false) || flavor["osType"] == "windows" {
            continue;
        }
        let code = match flavor["planCodes"]["hourly"].as_str() {
            Some(code) => code.to_string(),
            None => format!("{}.consumption", text(&flavor["name"])),
        };
        let Some(plan) = plans.get(&code) else {
            continue;
        };
        let Some(hourly) = array(&plan["pricings"])
            .iter()
            .find(|pricing| pricing["intervalUnit"] == "hour")
            .map(|pricing| number(&pricing["price"]) / 1e8)
        else {
            continue;
        };
        let memory_gib = number(&plan["blobs"]["technical"]["memory"]["size"]);
        offers.push(Offer {
            server_type: text(&flavor["name"]),
            location: text(&flavor["region"]),
            arch: "x86".to_string(),
            vcpus: number(&flavor["vcpus"]) as u32,
            memory_mib: if memory_gib > 0.0 {
                (memory_gib * 1024.0).round() as u64
            } else {
                number(&flavor["ram"]) as u64
            },
            disk_gib: number(&flavor["disk"]) as u64,
            price: hourly * HOURS_PER_MONTH,
        });
    }
    offers
}

/// An OVHcloud instance. A single instance carries its flavor; in a list it
/// has only `flavorId`, looked up in `flavors`.
fn ovh_server(instance: &Value, flavors: &[Value]) -> Server {
    let flavor = match &instance["flavor"] {
        Value::Null => flavors
            .iter()
            .find(|flavor| flavor["id"] == instance["flavorId"])
            .unwrap_or(&NULL),
        flavor => flavor,
    };
    let ipv4 = array(&instance["ipAddresses"])
        .iter()
        .find(|addr| addr["type"] == "public" && addr["version"] == 4)
        .and_then(|addr| addr["ip"].as_str())
        .map(str::to_string);
    Server {
        id: text(&instance["id"]),
        name: text(&instance["name"]),
        status: text(&instance["status"]),
        server_type: text(&flavor["name"]),
        location: text(&instance["region"]),
        ipv4,
        vcpus: number(&flavor["vcpus"]) as u64,
        memory_mib: number(&flavor["ram"]) as u64,
        disk_gib: number(&flavor["disk"]) as u64,
    }
}

/// A client for the provider's REST API, over [`crate::http`].
struct Api {
    provider: Provider,
    base: String,
    auth: Auth,
}

enum Auth {
    /// Hetzner's and DigitalOcean's API token.
    Bearer(String),
    Ovh(OvhKeys),
}

/// OVHcloud signs each request with application keys rather than sending a
/// token, and its VMs live in a Public Cloud project.
struct OvhKeys {
    application_key: String,
    application_secret: String,
    consumer_key: String,
    project: String,
    /// Whose public catalog prices the flavors: FR, CA or US.
    subsidiary: String,
    /// OVHcloud's clock minus ours, in seconds; signatures carry its time.
    clock_skew: i64,
}

impl OvhKeys {
    /// The `X-Ovh-*` headers that authenticate one request.
    fn headers(&self, method: &str, url: &str, body: &str) -> Vec<String> {
        let timestamp = unix_time() + self.clock_skew;
        let signature = ovh_signature(
            &self.application_secret,
            &self.consumer_key,
            method,
            url,
            body,
            timestamp,
        );
        vec![
            format!("X-Ovh-Application: {}", self.application_key),
            format!("X-Ovh-Consumer: {}", self.consumer_key),
            format!("X-Ovh-Timestamp: {timestamp}"),
            format!("X-Ovh-Signature: {signature}"),
        ]
    }
}

/// `$1$` and the SHA-1 of the secret, consumer key, method, full URL, body
/// and timestamp joined by `+`.
fn ovh_signature(
    secret: &str,
    consumer_key: &str,
    method: &str,
    url: &str,
    body: &str,
    timestamp: i64,
) -> String {
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(format!(
        "{secret}+{consumer_key}+{method}+{url}+{body}+{timestamp}"
    ));
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("$1${hex}")
}

fn unix_time() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// The API root and catalog subsidiary of an `OVH_ENDPOINT`, named as the
/// OVHcloud SDKs name them.
fn ovh_endpoint(name: &str) -> Result<(&'static str, &'static str)> {
    Ok(match name {
        "ovh-eu" => ("https://eu.api.ovh.com/1.0", "FR"),
        "ovh-ca" => ("https://ca.api.ovh.com/1.0", "CA"),
        "ovh-us" => ("https://api.us.ovhcloud.com/1.0", "US"),
        other => bail!("unknown OVHcloud endpoint '{other}' (ovh-eu, ovh-ca or ovh-us)"),
    })
}

/// Credentials from the environment, else from the current repository's
/// secret store, where `uc cloud` finds them too. The store is read once, on
/// the first credential the environment does not have.
#[derive(Default)]
struct Credentials {
    secrets: Option<Value>,
}

impl Credentials {
    fn get(&mut self, env: &str, key: &str) -> Result<String> {
        self.find(env, key)?.with_context(|| {
            format!(
                "no {key} in the {} secrets (or set {env})",
                crate::repo::fqn()
            )
        })
    }

    fn find(&mut self, env: &str, key: &str) -> Result<Option<String>> {
        if let Some(value) = std::env::var(env).ok().filter(|value| !value.is_empty()) {
            return Ok(Some(value));
        }
        if self.secrets.is_none() {
            let repo = crate::repo::fqn();
            let secrets = crate::secret::Store::new(&crate::secret::dir(), &repo)
                .get()
                .with_context(|| format!("reading the {repo} secrets (or set {env})"))?;
            self.secrets = Some(secrets);
        }
        Ok(self
            .secrets
            .as_ref()
            .and_then(|secrets| crate::secret::field(secrets, key))
            .filter(|value| value.is_string())
            .map(crate::secret::raw))
    }
}

impl Api {
    /// Hetzner's token is `HCLOUD_TOKEN` or `hetzner.prod` in the secrets,
    /// DigitalOcean's `DIGITALOCEAN_ACCESS_TOKEN` or `doctl.prod`. OVHcloud
    /// takes its keys, project and endpoint from `OVH_*` or `ovh.prod.*`.
    fn connect(provider: Provider) -> Result<Api> {
        let mut creds = Credentials::default();
        let (base, auth) = match provider {
            Provider::Hetzner => (
                "https://api.hetzner.cloud/v1",
                Auth::Bearer(creds.get("HCLOUD_TOKEN", "hetzner.prod")?),
            ),
            Provider::DigitalOcean => (
                "https://api.digitalocean.com/v2",
                Auth::Bearer(creds.get("DIGITALOCEAN_ACCESS_TOKEN", "doctl.prod")?),
            ),
            Provider::Ovh => {
                let endpoint = creds
                    .find("OVH_ENDPOINT", "ovh.prod.endpoint")?
                    .unwrap_or_else(|| "ovh-eu".to_string());
                let (base, subsidiary) = ovh_endpoint(&endpoint)?;
                let keys = OvhKeys {
                    application_key: creds
                        .get("OVH_APPLICATION_KEY", "ovh.prod.application_key")?,
                    application_secret: creds
                        .get("OVH_APPLICATION_SECRET", "ovh.prod.application_secret")?,
                    consumer_key: creds.get("OVH_CONSUMER_KEY", "ovh.prod.consumer_key")?,
                    project: creds.get("OVH_CLOUD_PROJECT_SERVICE", "ovh.prod.project")?,
                    subsidiary: subsidiary.to_string(),
                    clock_skew: ovh_clock_skew(base)?,
                };
                (base, Auth::Ovh(keys))
            }
        };
        Ok(Api {
            provider,
            base: base.to_string(),
            auth,
        })
    }

    /// OVHcloud's keys; only its API has them.
    fn ovh(&self) -> &OvhKeys {
        match &self.auth {
            Auth::Ovh(keys) => keys,
            Auth::Bearer(_) => unreachable!("{} has no OVHcloud keys", self.provider.name()),
        }
    }

    /// `path` under the OVHcloud Public Cloud project.
    fn project_path(&self, path: &str) -> String {
        format!("/cloud/project/{}{path}", self.ovh().project)
    }

    /// Sends one request and returns the JSON of a 2xx response (`null`
    /// when it has no body).
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        let body = body.map(Value::to_string);
        let mut headers = vec![
            "Content-Type: application/json".to_string(),
            "User-Agent: uc-vm".to_string(),
        ];
        match &self.auth {
            Auth::Bearer(token) => headers.push(format!("Authorization: Bearer {token}")),
            Auth::Ovh(keys) => {
                headers.extend(keys.headers(method, &url, body.as_deref().unwrap_or("")))
            }
        }
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

/// OVHcloud's time (`GET /auth/time`, unauthenticated) less the local one.
fn ovh_clock_skew(base: &str) -> Result<i64> {
    let url = format!("{base}/auth/time");
    let response = http::request("GET", &url, &[], None)?;
    let time: i64 = response
        .body
        .trim()
        .parse()
        .with_context(|| format!("GET {url}: HTTP {}: {}", response.status, response.body))?;
    Ok(time - unix_time())
}

/// Whether a list response has another page: Hetzner's
/// `meta.pagination.next_page`, DigitalOcean's `links.pages.next`.
fn has_next_page(body: &Value) -> bool {
    !body["meta"]["pagination"]["next_page"].is_null() || body["links"]["pages"]["next"].is_string()
}

/// The message of an API error: Hetzner's `error.message`, DigitalOcean's
/// and OVHcloud's `message`, or the body itself.
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
        Provider::Ovh => {
            let catalog = format!(
                "/order/catalog/public/cloud?ovhSubsidiary={}",
                api.ovh().subsidiary
            );
            ovh_offers(
                &api.call("GET", &api.project_path("/flavor"), None)?,
                &api.call("GET", &catalog, None)?,
            )
        }
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
        Provider::Ovh => {
            let flavors = api.call("GET", &api.project_path("/flavor"), None)?;
            let instances = api.call("GET", &api.project_path("/instance"), None)?;
            array(&instances)
                .iter()
                .map(|instance| ovh_server(instance, array(&flavors)))
                .collect()
        }
    })
}

/// One server, fresh from the API.
fn get(api: &Api, id: &str) -> Result<Server> {
    Ok(match api.provider {
        Provider::Hetzner => {
            hetzner_server(&api.call("GET", &format!("/servers/{id}"), None)?["server"])
        }
        Provider::DigitalOcean => {
            digitalocean_server(&api.call("GET", &format!("/droplets/{id}"), None)?["droplet"])
        }
        Provider::Ovh => ovh_server(
            &api.call("GET", &api.project_path(&format!("/instance/{id}")), None)?,
            &[],
        ),
    })
}

/// The `POST /cloud/project/<project>/instance` body. The instance is billed
/// by the hour, as the flavors are priced: OVHcloud's monthly plan commits
/// to the whole month.
fn ovh_instance(name: &str, flavor: &Value, image: &Value, region: &str, user_data: &str) -> Value {
    json!({
        "name": name,
        "flavorId": flavor,
        "imageId": image,
        "region": region,
        "userData": user_data,
        "monthlyBilling": false,
    })
}

/// Polls a server until `ready` holds it, for the APIs whose requests start
/// no action to wait on.
fn wait_for(api: &Api, id: &str, ready: impl Fn(&Server) -> bool) -> Result<Server> {
    for _ in 0..POLL_TRIES {
        let server = get(api, id)?;
        if ready(&server) {
            return Ok(server);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    bail!("timed out waiting for {} server {id}", api.provider.name())
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
    if provider != Provider::Hetzner && !spec.start {
        bail!(
            "{} servers always start when created; --no-start is not supported",
            provider.name()
        );
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
                wait_for(api, &droplet.id, |s| {
                    s.status == "active" && s.ipv4.is_some()
                })?
            } else {
                droplet
            }
        }
        Provider::Ovh => {
            let region = &offer.location;
            let flavors = api.call(
                "GET",
                &api.project_path(&format!("/flavor?region={region}")),
                None,
            )?;
            let flavor = array(&flavors)
                .iter()
                .find(|flavor| flavor["name"] == offer.server_type.as_str())
                .with_context(|| format!("flavor {} not found in {region}", offer.server_type))?;
            let images = api.call(
                "GET",
                &api.project_path(&format!("/image?osType=linux&region={region}")),
                None,
            )?;
            let found = array(&images)
                .iter()
                .find(|found| text(&found["name"]).eq_ignore_ascii_case(image))
                .with_context(|| {
                    let names: Vec<String> = array(&images)
                        .iter()
                        .map(|img| text(&img["name"]))
                        .collect();
                    format!(
                        "no image '{image}' in {region}; there are: {}",
                        names.join(", ")
                    )
                })?;
            let body = ovh_instance(&spec.name, &flavor["id"], &found["id"], region, &user_data);
            let instance = api.call("POST", &api.project_path("/instance"), Some(&body))?;
            let id = text(&instance["id"]);
            if spec.wait_for_ip {
                wait_for(api, &id, |s| s.status == "ACTIVE" && s.ipv4.is_some())?
            } else {
                ovh_server(&instance, &[])
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
    match api.provider {
        Provider::Hetzner => {
            let verb = match action {
                Power::Start => "poweron",
                Power::Shutdown => "shutdown",
                Power::Off => "poweroff",
                Power::Reboot => "reboot",
                Power::Reset => "reset",
            };
            api.wait(&api.call("POST", &format!("/servers/{id}/actions/{verb}"), None)?)?;
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
            api.wait(&api.call("POST", &format!("/droplets/{id}/actions"), Some(&body))?)?;
        }
        Provider::Ovh => {
            // OVHcloud has no graceful stop, and no action to wait on: the
            // instance's status says when it is done.
            let (verb, body, status) = match action {
                Power::Start => ("start", None, "ACTIVE"),
                Power::Shutdown | Power::Off => ("stop", None, "SHUTOFF"),
                Power::Reboot => ("reboot", Some(json!({ "type": "soft" })), "ACTIVE"),
                Power::Reset => ("reboot", Some(json!({ "type": "hard" })), "ACTIVE"),
            };
            let path = api.project_path(&format!("/instance/{id}/{verb}"));
            api.call("POST", &path, body.as_ref())?;
            wait_for(api, id, |s| s.status == status)?;
        }
    }
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
        Provider::Ovh => drop(api.call(
            "DELETE",
            &api.project_path(&format!("/instance/{id}")),
            None,
        )?),
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
        let (provider, _) = split_provider(&strings(&["list", "--provider", "ovh"])).unwrap();
        assert_eq!(provider, Some(Provider::Ovh));
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
    fn ovh_flavors_are_priced_from_the_catalog() {
        let flavors = json!([
            {"id": "f1", "name": "b3-8", "region": "GRA11", "vcpus": 2, "ram": 8000,
             "disk": 50, "osType": "linux", "available": true,
             "planCodes": {"hourly": "b3-8.consumption", "monthly": null}},
            {"id": "f2", "name": "win-b3-8", "region": "GRA11", "vcpus": 2, "ram": 8000,
             "disk": 50, "osType": "windows", "available": true,
             "planCodes": {"hourly": "win-b3-8.consumption"}},
            {"id": "f3", "name": "unpriced", "region": "GRA11", "vcpus": 2, "ram": 8000,
             "disk": 50, "osType": "linux", "available": true, "planCodes": {}}
        ]);
        let catalog = json!({"addons": [
            {"planCode": "b3-8.consumption",
             "pricings": [{"price": 5120000, "intervalUnit": "hour"}],
             "blobs": {"technical": {"memory": {"size": 8}}}},
            {"planCode": "win-b3-8.consumption",
             "pricings": [{"price": 1, "intervalUnit": "hour"}]}
        ]});
        let offers = ovh_offers(&flavors, &catalog);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].location, "GRA11");
        assert_eq!(offers[0].memory_mib, 8192);
        assert!((offers[0].price - 0.0512 * 730.0).abs() < 1e-9);
    }

    #[test]
    fn ovh_instances_are_billed_by_the_hour() {
        let body = ovh_instance(
            "uc4",
            &json!("f1"),
            &json!("i1"),
            "GRA11",
            "#cloud-config\n",
        );
        assert_eq!(body["monthlyBilling"], false);
        assert_eq!(body["flavorId"], "f1");
    }

    #[test]
    fn ovh_requests_are_signed() {
        assert_eq!(
            ovh_signature(
                "AS",
                "CK",
                "GET",
                "https://eu.api.ovh.com/1.0/cloud/project/p/instance",
                "",
                1790489276
            ),
            "$1$88c229378506bf49b4a98a62de074c2fcf4b6c5a"
        );
        assert_eq!(
            ovh_endpoint("ovh-ca").unwrap(),
            ("https://ca.api.ovh.com/1.0", "CA")
        );
        assert!(ovh_endpoint("kimsufi-eu").is_err());
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

        let flavors = [json!({"id": "f1", "name": "b3-8", "vcpus": 2, "ram": 8000, "disk": 50})];
        let instance = ovh_server(
            &json!({
                "id": "abc", "name": "uc4", "status": "ACTIVE", "region": "GRA11",
                "flavorId": "f1",
                "ipAddresses": [{"ip": "2001:db8::1", "type": "public", "version": 6},
                                {"ip": "9.9.9.9", "type": "public", "version": 4}]
            }),
            &flavors,
        );
        assert_eq!(instance.server_type, "b3-8");
        assert_eq!(instance.vcpus, 2);
        assert_eq!(instance.ipv4.as_deref(), Some("9.9.9.9"));
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
