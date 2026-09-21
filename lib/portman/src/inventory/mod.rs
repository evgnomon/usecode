//! Reads and extends the mesh topology, which lives in one place: a
//! multi-file Ansible inventory under deploy/inventory.
//!
//! The topology is the single source of truth for who is in the mesh and
//! what address each host holds. Nothing allocates an address on the
//! host itself any more, which is what stops two hosts from ever being
//! handed the same one: `portman add` looks at every address already
//! recorded here and picks the lowest free one in the configured
//! network.
//!
//! The layout is:
//!
//! ```text
//! deploy/inventory/hosts.yml                       group membership
//! deploy/inventory/group_vars/portman/main.yml     mesh-wide inputs
//! deploy/inventory/group_vars/portman/secrets.yml  vaulted private keys
//! deploy/inventory/host_vars/<host>.yml            one host's unique facts
//! ```
//!
//! Only hosts.yml and host_vars/<host>.yml are written by portman, and
//! only ever by adding to them: a host file is created once, when the
//! host joins, and is yours to hand-edit afterwards.

mod vault;
mod yamledit;

pub use vault::{check_available, Vault};

use std::collections::BTreeMap;
use std::fs;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Context, Result};
use crate::keys;
use crate::net::{next_addr, Prefix};

/// The inventory group whose members portman manages.
pub const GROUP: &str = "portman";

/// Where the inventory lives inside a portman checkout, relative to the
/// repository root.
pub const DEFAULT_DIR: &str = "deploy/inventory";

/// The mesh-wide inputs, read from group_vars/portman/main.yml. These
/// are the knobs a human sets; every per-host value is derived from
/// them.
#[derive(Debug, Default, Deserialize)]
pub struct Settings {
    /// The CIDR every host's tunnel address is drawn from.
    #[serde(rename = "portman_network", default)]
    pub network: String,
    /// The first host number in `network` that may be allocated, so that
    /// e.g. .1 can be kept for a router if wanted.
    #[serde(rename = "portman_address_start", default)]
    pub address_start: i64,
    /// The WireGuard port, used to build the endpoint of a host added
    /// with a public address but no explicit port.
    #[serde(rename = "portman_listen_port", default)]
    pub listen_port: i64,
}

/// One mesh member's unique facts, stored in host_vars/<name>.yml.
/// Fields portman does not manage (extra Ansible vars, services added by
/// hand) are preserved because portman only ever creates this file,
/// never rewrites it.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Host {
    #[serde(skip)]
    pub name: String,

    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ansible_host: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ansible_user: String,

    /// This host's tunnel address without a prefix length, e.g.
    /// "10.10.0.3". The prefix comes from [`Settings::network`].
    #[serde(rename = "portman_address", default)]
    pub address: String,
    /// The WireGuard public key of the keypair minted for this host; its
    /// private half lives in the vault, never here.
    #[serde(rename = "portman_public_key", default)]
    pub public_key: String,
    /// host:port other members should dial to reach this host. Empty
    /// means this host only dials out (it is behind NAT).
    #[serde(
        rename = "portman_endpoint",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub endpoint: String,

    /// This host's `[[service]]` declarations, filled in by hand (or
    /// with `portman forward` on the host itself).
    #[serde(rename = "portman_services", default)]
    pub services: Vec<serde_yaml::Value>,
}

/// The topology as read off disk.
#[derive(Debug)]
pub struct Inventory {
    /// The inventory root, e.g. `<repo>/deploy/inventory`.
    pub dir: PathBuf,
    /// The directory ansible commands should run from - the repo root,
    /// two levels above [`Inventory::dir`], which is where ansible.cfg
    /// lives.
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
        let root = abs
            .parent()
            .and_then(Path::parent)
            .unwrap_or(&abs)
            .to_path_buf();

        let mut inv = Inventory {
            dir: abs,
            root,
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
            bail!("{}: portman_network is required", path.display());
        }

        for name in inv.host_names()? {
            let host = inv.load_host(&name)?;
            inv.hosts.push(host);
        }

        Ok(inv)
    }

    /// The members of the portman group in hosts.yml.
    fn host_names(&self) -> Result<Vec<String>> {
        let path = self.hosts_path();
        let body = fs::read_to_string(&path).with_ctx(|| format!("read {}", path.display()))?;

        // Only the portman group's host keys are needed, so walk a loose
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

        let mut host: Host =
            serde_yaml::from_str(&body).with_ctx(|| format!("parse {}", path.display()))?;
        host.name = name.to_string();
        if host.address.is_empty() {
            bail!("{}: portman_address is required", path.display());
        }
        Ok(host)
    }

    /// Look up a member by inventory hostname.
    pub fn host(&self, name: &str) -> Option<&Host> {
        self.hosts.iter().find(|h| h.name == name)
    }

    /// Record a new mesh member: allocate the host's tunnel address
    /// (unless one was given), mint its WireGuard keypair, write
    /// host_vars/<name>.yml, append the host to the portman group in
    /// hosts.yml, and store the private key in the vault. Returns the
    /// host as recorded.
    ///
    /// It is refused if the host is already in the inventory: re-adding
    /// would mean minting a second keypair for a host that already has
    /// one, which silently breaks its tunnel. Edit host_vars/<name>.yml
    /// instead.
    pub fn add(&mut self, req: NewHost, vault: &Vault) -> Result<Host> {
        valid_name(&req.name)?;
        if self.host(&req.name).is_some() {
            bail!(
                "{} is already in the mesh (see {})",
                req.name,
                self.host_vars_path(&req.name).display()
            );
        }
        if self.host_vars_path(&req.name).exists() {
            bail!(
                "{} already exists but {} is not in the {GROUP} group; add it there or delete \
                 the file",
                self.host_vars_path(&req.name).display(),
                req.name
            );
        }

        let address = if req.address.is_empty() {
            self.allocate()?
        } else {
            self.check_address(&req.address)?;
            req.address.clone()
        };

        let (private_key, public_key) = keys::new_pair()?;

        let host = Host {
            name: req.name.clone(),
            ansible_host: req.ansible_host,
            ansible_user: req.ansible_user,
            address,
            public_key,
            endpoint: self.endpoint(&req.endpoint),
            services: Vec::new(),
        };

        // The private key goes in first: a vault write that fails (wrong
        // password, no ansible-vault) leaves the inventory untouched
        // rather than leaving a host with no key behind.
        vault.put(&host.name, &private_key)?;
        self.write_host_vars(&host)?;
        self.add_to_group(&host.name)?;

        self.hosts.push(host.clone());
        self.hosts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(host)
    }

    /// Complete a dialable address: given just a host or IP, append the
    /// mesh's WireGuard port, since that is the only port an endpoint
    /// could mean.
    fn endpoint(&self, value: &str) -> String {
        if value.is_empty() || value.contains(':') {
            return value.to_string();
        }
        let port = match self.settings.listen_port {
            0 => 51820,
            p => p,
        };
        format!("{value}:{port}")
    }

    /// The lowest address in the configured network that no host holds
    /// yet. This is the whole point of a central topology: the answer
    /// depends on every other host, so it cannot be decided on the host
    /// being added.
    fn allocate(&self) -> Result<String> {
        let prefix = self.network()?;

        let mut taken: BTreeMap<Ipv4Addr, &str> = BTreeMap::new();
        for h in &self.hosts {
            let addr: Ipv4Addr = h.address.parse().with_ctx(|| {
                format!(
                    "{}: portman_address {:?} is not an IP address",
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
            "no free address left in portman_network {} ({} hosts); widen the network in {}",
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
                "-address {addr} is outside portman_network {}",
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
                "{}: portman_network {:?} must be an IPv4 CIDR (e.g. 10.10.0.0/24)",
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

        let header = format!(
            "---\n\
             # {name} - one member of the portman mesh.\n\
             #\n\
             # Created by `portman add {name}`, and not touched by portman again:\n\
             # edit it freely. portman_address was allocated from portman_network in\n\
             # group_vars/{GROUP}/main.yml, and the private key half of\n\
             # portman_public_key is in group_vars/{GROUP}/secrets.yml under this host's\n\
             # name.\n",
            name = host.name
        );

        let path = self.host_vars_path(&host.name);
        let dir = path.parent().unwrap_or(&self.dir);
        fs::create_dir_all(dir).with_ctx(|| format!("create {}", dir.display()))?;
        fs::write(&path, header + &body).with_ctx(|| format!("write {}", path.display()))?;
        Ok(())
    }

    /// Append `name` to the portman group in hosts.yml, editing the text
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
