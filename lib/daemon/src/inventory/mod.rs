// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Reads and extends the mesh topology, which lives in one place: a
//! multi-file Ansible inventory under ~/.config/usecode/inventory. It is
//! your own configuration, not part of the checkout.
//!
//! The topology is the single source of truth for who is in the mesh and
//! what address each host holds. Nothing allocates an address on the
//! host itself any more, which is what stops two hosts from ever being
//! handed the same one: `uc net mesh add` looks at every address already
//! recorded here and picks the lowest free one in the configured
//! network.
//!
//! The layout is:
//!
//! ```text
//! ~/.config/usecode/inventory/hosts.yml                       group membership
//! ~/.config/usecode/inventory/group_vars/usecode/main.yml     mesh-wide inputs
//! ~/.config/usecode/inventory/group_vars/usecode/secrets.yml  vaulted private keys
//! ~/.config/usecode/inventory/host_vars/<host>.yml            one host's unique facts
//! ```
//!
//! Only hosts.yml and host_vars/<host>.yml are written by uc net mesh, and
//! only ever by adding to them: a host file is created once, when the
//! host is installed or joins, and is yours to hand-edit afterwards. The
//! one later write is turning the mesh on for an installed host, which
//! appends its mesh facts to the end of the file.
//!
//! A host is in the inventory before it is in the mesh: `uc daemon
//! install` records it with no address or key, and the mesh only starts
//! for it once `uc net mesh add` sets `usecode_mesh_enabled`.

mod render;
mod vault;
mod yamledit;

pub use vault::{Vault, check_available};

use std::collections::BTreeMap;
use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Context, Result};
use crate::keys;
use crate::net::{Prefix, next_addr};

/// The inventory group whose members uc net mesh manages.
pub const GROUP: &str = "usecode";

/// Where the inventory lives, relative to the usecode config directory
/// ($XDG_CONFIG_HOME/usecode, or ~/.config/usecode).
pub const DEFAULT_DIR: &str = "inventory";

/// The mesh-wide inputs, read from group_vars/usecode/main.yml. These
/// are the knobs a human sets; every per-host value is derived from
/// them.
#[derive(Debug, Default, Deserialize)]
pub struct Settings {
    /// The CIDR every host's tunnel address is drawn from.
    #[serde(rename = "usecode_network", default)]
    pub network: String,
    /// The first host number in `network` that may be allocated, so that
    /// e.g. .1 can be kept for a router if wanted.
    #[serde(rename = "usecode_address_start", default)]
    pub address_start: i64,
    /// The WireGuard port, used to build the endpoint of a host added
    /// with a public address but no explicit port.
    #[serde(rename = "usecode_listen_port", default)]
    pub listen_port: i64,
    /// The WireGuard interface on every member.
    #[serde(rename = "usecode_interface", default)]
    pub interface: String,
    /// How often a member pings a peer it dials, to keep a NAT mapping
    /// open.
    #[serde(rename = "usecode_persistent_keepalive", default)]
    pub persistent_keepalive: Option<i64>,
    /// The tunnel MTU; 0 leaves it to the kernel.
    #[serde(rename = "usecode_mtu", default)]
    pub mtu: i64,
}

/// One host's unique facts, stored in host_vars/<name>.yml.
/// Fields uc net mesh does not manage (extra Ansible vars, services added by
/// hand) are preserved because uc net mesh only ever creates this file,
/// never rewrites it.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Host {
    #[serde(skip)]
    pub name: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ansible_host: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ansible_user: String,
    /// "local" for the control node itself, which is set up without ssh.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ansible_connection: String,

    /// Whether this host is in the mesh. Off for a host that only has
    /// the daemon installed; `uc net mesh add` turns it on together with
    /// the address and key below.
    #[serde(
        rename = "usecode_mesh_enabled",
        default,
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub mesh_enabled: bool,
    /// This host's tunnel address without a prefix length, e.g.
    /// "10.10.0.3". The prefix comes from [`Settings::network`]. Empty
    /// until the host joins the mesh.
    #[serde(
        rename = "usecode_address",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub address: String,
    /// The WireGuard public key of the keypair minted for this host; its
    /// private half lives in the vault, never here.
    #[serde(
        rename = "usecode_public_key",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub public_key: String,
    /// host:port other members should dial to reach this host. Empty
    /// means this host only dials out (it is behind NAT).
    #[serde(
        rename = "usecode_endpoint",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub endpoint: String,

    /// This host's `[[service]]` declarations, filled in by hand (or
    /// with `uc net mesh forward` on the host itself).
    #[serde(rename = "usecode_services", default)]
    pub services: Vec<serde_yaml::Value>,
}

/// The topology as read off disk.
#[derive(Debug)]
pub struct Inventory {
    /// The inventory root, e.g. `~/.config/usecode/inventory`.
    pub dir: PathBuf,
    /// The directory ansible commands should run from - lib/daemon of
    /// the usecode checkout, which is where ansible.cfg lives.
    pub root: PathBuf,

    pub settings: Settings,
    pub hosts: Vec<Host>,
}

/// The request to put a host into the mesh. Every field is optional
/// except `name`; `address` is allocated when left empty.
#[derive(Debug, Default)]
pub struct NewHost {
    pub name: String,
    pub address: String,
    pub endpoint: String,
    pub ansible_host: String,
    pub ansible_user: String,
}

/// The inventory directory: $XDG_CONFIG_HOME/usecode/inventory, or
/// ~/.config/usecode/inventory.
pub fn find() -> Result<PathBuf> {
    let config = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var("HOME").ctx("HOME is not set")?).join(".config"),
    };
    Ok(config.join("usecode").join(DEFAULT_DIR))
}

/// Start an empty inventory at `dir` - no hosts, the default mesh
/// settings - unless one is already there. Never touches an existing one.
pub fn init(dir: &Path) -> Result<()> {
    let hosts = dir.join("hosts.yml");
    if hosts.exists() {
        return Ok(());
    }
    let settings = dir.join("group_vars").join(GROUP).join("main.yml");
    fs::create_dir_all(settings.parent().expect("has a parent"))
        .with_ctx(|| format!("create {}", dir.display()))?;
    if !settings.exists() {
        fs::write(&settings, include_str!("skel/main.yml"))
            .with_ctx(|| format!("write {}", settings.display()))?;
    }
    fs::write(&hosts, include_str!("skel/hosts.yml"))
        .with_ctx(|| format!("write {}", hosts.display()))?;
    println!("started a new inventory at {}", dir.display());
    Ok(())
}

/// lib/daemon of the usecode checkout, where ansible.cfg lives: looked
/// for from the current directory up, falling back to the checkout this
/// binary was built from.
pub fn checkout() -> PathBuf {
    let built_from = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let Ok(mut dir) = std::env::current_dir() else {
        return built_from;
    };
    loop {
        for base in [dir.clone(), dir.join("lib/daemon")] {
            if base.join("ansible.cfg").exists() && base.join("Cargo.toml").exists() {
                return base;
            }
        }
        if !dir.pop() {
            return built_from;
        }
    }
}

impl Inventory {
    fn hosts_path(&self) -> PathBuf {
        self.dir.join("hosts.yml")
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("group_vars").join(GROUP).join("main.yml")
    }

    fn host_vars_path(&self, name: &str) -> PathBuf {
        self.dir.join("host_vars").join(format!("{name}.yml"))
    }

    /// The vaulted file holding every host's private key.
    pub fn secrets_path(&self) -> PathBuf {
        self.dir.join("group_vars").join(GROUP).join("secrets.yml")
    }

    /// Read the inventory rooted at `dir`.
    pub fn load(dir: &Path) -> Result<Inventory> {
        let abs = fs::canonicalize(dir)
            .or_else(|_| std::env::current_dir().map(|cwd| cwd.join(dir)))
            .with_ctx(|| format!("resolve {}", dir.display()))?;
        let mut inv = Inventory {
            dir: abs,
            root: checkout(),
            settings: Settings::default(),
            hosts: Vec::new(),
        };

        if !inv.hosts_path().exists() {
            bail!(
                "no inventory at {} (expected {}); pass -inventory DIR",
                dir.display(),
                inv.hosts_path().display()
            );
        }

        let path = inv.settings_path();
        let body = fs::read_to_string(&path).with_ctx(|| format!("read {}", path.display()))?;
        inv.settings =
            serde_yaml::from_str(&body).with_ctx(|| format!("parse {}", path.display()))?;
        if inv.settings.network.is_empty() {
            bail!("{}: usecode_network is required", path.display());
        }

        for name in inv.host_names()? {
            let host = inv.load_host(&name)?;
            inv.hosts.push(host);
        }

        Ok(inv)
    }

    /// The members of the usecode group in hosts.yml.
    fn host_names(&self) -> Result<Vec<String>> {
        let path = self.hosts_path();
        let body = fs::read_to_string(&path).with_ctx(|| format!("read {}", path.display()))?;

        // Only the usecode group's host keys are needed, so walk a loose
        // value rather than modelling Ansible's whole inventory schema.
        let doc: serde_yaml::Value =
            serde_yaml::from_str(&body).with_ctx(|| format!("parse {}", path.display()))?;

        let hosts = ["all", "children", GROUP, "hosts"]
            .iter()
            .try_fold(&doc, |node, key| node.get(key));

        let mut names: Vec<String> = match hosts.and_then(serde_yaml::Value::as_mapping) {
            Some(mapping) => mapping
                .keys()
                .filter_map(|k| k.as_str().map(String::from))
                .collect(),
            None => Vec::new(),
        };
        names.sort();
        Ok(names)
    }

    fn load_host(&self, name: &str) -> Result<Host> {
        let path = self.host_vars_path(name);
        let body = match fs::read_to_string(&path) {
            Ok(body) => body,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!(
                    "{name} is in the {GROUP} group but {} is missing; add it or remove the \
                     host from {}",
                    path.display(),
                    self.hosts_path().display()
                )
            }
            Err(e) => bail!("read {}: {e}", path.display()),
        };

        // An empty file is a host with nothing set, not a parse error.
        let mut host: Host = match serde_yaml::from_str::<Option<Host>>(&body) {
            Ok(host) => host.unwrap_or_default(),
            Err(e) => bail!("parse {}: {e}", path.display()),
        };
        host.name = name.to_string();
        if host.mesh_enabled && host.address.is_empty() {
            bail!(
                "{}: usecode_mesh_enabled is set but usecode_address is not",
                path.display()
            );
        }
        Ok(host)
    }

    /// Look up a member by inventory hostname.
    pub fn host(&self, name: &str) -> Option<&Host> {
        self.hosts.iter().find(|h| h.name == name)
    }

    /// Record a host with the daemon only - no address, no key, the
    /// mesh off: write host_vars/<name>.yml with how to reach it and
    /// append it to the usecode group in hosts.yml. This needs nothing
    /// from the vault. Returns the host as recorded.
    pub fn add_host(&mut self, req: NewHost) -> Result<Host> {
        self.check_new(&req.name)?;

        let host = Host {
            name: req.name,
            ansible_host: req.ansible_host,
            ansible_user: req.ansible_user,
            ..Host::default()
        };
        self.write_host_vars(&host)?;
        self.add_to_group(&host.name)?;

        self.hosts.push(host.clone());
        self.hosts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(host)
    }

    /// Put a host into the mesh: allocate its tunnel address (unless one
    /// was given), mint its WireGuard keypair, store the private key in
    /// the vault, and record the rest in host_vars/<name>.yml. A host
    /// that isn't in the inventory yet is added to it; one that already
    /// is, with the mesh off (installed with `uc daemon install`), has
    /// its mesh facts appended to its host_vars file. Returns the host as
    /// recorded.
    ///
    /// It is refused if the host is already in the mesh: re-adding would
    /// mean minting a second keypair for a host that already has one,
    /// which silently breaks its tunnel. Edit host_vars/<name>.yml
    /// instead.
    pub fn add(&mut self, req: NewHost, vault: &Vault) -> Result<Host> {
        let existing = self.host(&req.name).cloned();
        match &existing {
            Some(h) if h.mesh_enabled || !h.address.is_empty() => bail!(
                "{} is already in the mesh (see {})",
                req.name,
                self.host_vars_path(&req.name).display()
            ),
            Some(_) => {}
            None => self.check_new(&req.name)?,
        }

        let address = if req.address.is_empty() {
            self.allocate()?
        } else {
            self.check_address(&req.address)?;
            req.address.clone()
        };

        let (private_key, public_key) = keys::new_pair()?;

        let endpoint = self.endpoint(&req.endpoint);
        let host = match existing {
            Some(h) => Host {
                mesh_enabled: true,
                address,
                public_key,
                endpoint,
                ..h
            },
            None => Host {
                name: req.name.clone(),
                ansible_host: req.ansible_host,
                ansible_user: req.ansible_user,
                mesh_enabled: true,
                address,
                public_key,
                endpoint,
                ..Host::default()
            },
        };

        // The private key goes in first: a vault write that fails (wrong
        // password, no ansible-vault) leaves the inventory untouched
        // rather than leaving a host with no key behind.
        vault.put(&host.name, &private_key)?;
        if self.host(&host.name).is_some() {
            self.append_mesh_vars(&host)?;
            self.hosts.retain(|h| h.name != host.name);
        } else {
            self.write_host_vars(&host)?;
            self.add_to_group(&host.name)?;
        }

        self.hosts.push(host.clone());
        self.hosts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(host)
    }

    /// Refuse a name that can't be a new host: unusable as a file name,
    /// already in the inventory, or with a host_vars file left behind.
    fn check_new(&self, name: &str) -> Result<()> {
        valid_name(name)?;
        if self.host(name).is_some() {
            bail!(
                "{name} is already in the inventory (see {})",
                self.host_vars_path(name).display()
            );
        }
        if self.host_vars_path(name).exists() {
            bail!(
                "{} already exists but {name} is not in the {GROUP} group; add it there or \
                 delete the file",
                self.host_vars_path(name).display()
            );
        }
        Ok(())
    }

    /// Complete a dialable address: given just a host or IP, append the
    /// mesh's WireGuard port, since that is the only port an endpoint
    /// could mean.
    fn endpoint(&self, value: &str) -> String {
        if value.is_empty() || value.contains(':') {
            return value.to_string();
        }
        format!("{value}:{}", self.listen_port())
    }

    /// The lowest address in the configured network that no host holds
    /// yet. This is the whole point of a central topology: the answer
    /// depends on every other host, so it cannot be decided on the host
    /// being added.
    fn allocate(&self) -> Result<String> {
        let prefix = self.network()?;

        let mut taken: BTreeMap<Ipv4Addr, &str> = BTreeMap::new();
        for h in self.hosts.iter().filter(|h| !h.address.is_empty()) {
            let addr: Ipv4Addr = h.address.parse().with_ctx(|| {
                format!(
                    "{}: usecode_address {:?} is not an IP address",
                    h.name, h.address
                )
            })?;
            if let Some(other) = taken.get(&addr) {
                bail!(
                    "{other} and {} both claim {addr}; fix the topology before adding hosts",
                    h.name
                );
            }
            taken.insert(addr, &h.name);
        }

        let mut addr = prefix.host_number(self.settings.address_start);
        while prefix.contains(addr) {
            if prefix.is_broadcast(addr) {
                break;
            }
            if !taken.contains_key(&addr) {
                return Ok(addr.to_string());
            }
            match next_addr(addr) {
                Some(next) => addr = next,
                None => break,
            }
        }

        bail!(
            "no free address left in usecode_network {} ({} hosts); widen the network in {}",
            self.settings.network,
            self.hosts.len(),
            self.settings_path().display()
        )
    }

    /// Validate an operator-supplied address against the same rules
    /// allocation follows, so -address can't reintroduce a clash.
    fn check_address(&self, address: &str) -> Result<()> {
        let addr: Ipv4Addr = address.parse().with_ctx(|| {
            format!(
                "-address {address:?} is not an IP address (it takes a bare address, e.g. \
                 10.10.0.5)"
            )
        })?;
        let prefix = self.network()?;
        if !prefix.contains(addr) {
            bail!(
                "-address {addr} is outside usecode_network {}",
                self.settings.network
            );
        }
        for h in &self.hosts {
            if h.address == addr.to_string() {
                bail!("-address {addr} is already held by {}", h.name);
            }
        }
        Ok(())
    }

    fn network(&self) -> Result<Prefix> {
        Prefix::parse(&self.settings.network).with_ctx(|| {
            format!(
                "{}: usecode_network {:?} must be an IPv4 CIDR (e.g. 10.10.0.0/24)",
                self.settings_path().display(),
                self.settings.network
            )
        })
    }

    /// The network's prefix length, which is what turns a host's bare
    /// address into the CIDR WireGuard wants.
    pub fn prefix(&self) -> Result<u8> {
        Ok(self.network()?.bits())
    }

    fn write_host_vars(&self, host: &Host) -> Result<()> {
        let body = serde_yaml::to_string(host)
            .with_ctx(|| format!("encode host vars for {}", host.name))?;

        let header = if host.mesh_enabled {
            format!(
                "---\n\
                 # {name} - one member of the usecode mesh.\n\
                 #\n\
                 # Created by `uc net mesh add {name}`, and not touched by uc net mesh again:\n\
                 # edit it freely. {MESH_NOTE}",
                name = host.name
            )
        } else {
            format!(
                "---\n\
                 # {name} - a host running the usecode daemon, with the mesh off.\n\
                 #\n\
                 # Created by `uc daemon install {name}`: edit it freely. `uc net mesh add\n\
                 # {name}` turns the mesh on for it by appending its address and key here.\n",
                name = host.name
            )
        };

        let path = self.host_vars_path(&host.name);
        let dir = path.parent().unwrap_or(&self.dir);
        fs::create_dir_all(dir).with_ctx(|| format!("create {}", dir.display()))?;
        fs::write(&path, header + &body).with_ctx(|| format!("write {}", path.display()))?;
        Ok(())
    }

    /// Turn the mesh on for a host already in the inventory by appending
    /// its mesh facts to host_vars/<name>.yml. Appending top-level keys
    /// leaves everything already in the file - comments, hand edits -
    /// exactly as it was.
    fn append_mesh_vars(&self, host: &Host) -> Result<()> {
        let facts = Host {
            mesh_enabled: true,
            address: host.address.clone(),
            public_key: host.public_key.clone(),
            endpoint: host.endpoint.clone(),
            ..Host::default()
        };
        let body = serde_yaml::to_string(&facts)
            .with_ctx(|| format!("encode host vars for {}", host.name))?;
        // Only the mesh keys: services stay whatever the file already says.
        let body: String = body
            .lines()
            .filter(|l| !l.starts_with("usecode_services:"))
            .map(|l| format!("{l}\n"))
            .collect();

        let path = self.host_vars_path(&host.name);
        let mut text = fs::read_to_string(&path).with_ctx(|| format!("read {}", path.display()))?;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!(
            "\n# Mesh turned on by `uc net mesh add {}`. {MESH_NOTE}{body}",
            host.name
        ));
        fs::write(&path, text).with_ctx(|| format!("write {}", path.display()))?;
        Ok(())
    }

    /// Append `name` to the usecode group in hosts.yml, editing the text
    /// rather than re-encoding it, so the comments and layout of a
    /// hand-maintained inventory survive.
    fn add_to_group(&self, name: &str) -> Result<()> {
        let path = self.hosts_path();
        let text = fs::read_to_string(&path).with_ctx(|| format!("read {}", path.display()))?;

        let out = yamledit::insert_key(&text, &["all", "children", GROUP, "hosts"], name)
            .with_ctx(|| format!("{}", path.display()))?;

        fs::write(&path, out).with_ctx(|| format!("write {}", path.display()))?;
        Ok(())
    }
}

/// Where a mesh host's facts come from, for the comment above them.
const MESH_NOTE: &str = "usecode_address was allocated from\n\
# usecode_network in group_vars/usecode/main.yml, and the private key half\n\
# of usecode_public_key is in group_vars/usecode/secrets.yml under this\n\
# host's name.\n";

fn valid_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("a host name is required");
    }
    if name.contains(['/', '\\', ' ', '\t', ':']) || name == "." || name == ".." {
        bail!("{name:?} is not a usable inventory hostname");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
