// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The machine facts the configuration branches on, read straight from the
//! system in place of Ansible's fact gathering. Values use Ansible's spelling
//! (`Debian`, `x86_64`, `trixie`) so templates written for `ansible_facts`
//! render unchanged.

use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;

#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// `Debian`, `Ubuntu`, …
    pub distribution: String,
    /// The codename, e.g. `trixie`.
    pub distribution_release: String,
    pub distribution_version: String,
    pub distribution_major_version: String,
    /// `Debian` for Debian and its derivatives.
    pub os_family: String,
    /// `x86_64`, `aarch64`, …
    pub architecture: String,
    /// The kernel release, e.g. `6.12.0-1-amd64`.
    pub kernel: String,
    pub user_id: String,
    pub uid: u32,
    pub memtotal_mb: u64,
    pub env: BTreeMap<String, String>,
}

impl Facts {
    pub fn gather() -> Facts {
        let os_release = std::fs::read_to_string("/etc/os-release")
            .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
            .unwrap_or_default();
        let debian_version = std::fs::read_to_string("/etc/debian_version").unwrap_or_default();
        let mut facts = Facts::from_os_release(&os_release, &debian_version);
        facts.architecture = std::env::consts::ARCH.to_string();
        facts.kernel = read_trimmed("/proc/sys/kernel/osrelease");
        facts.memtotal_mb = std::fs::read_to_string("/proc/meminfo")
            .map(|m| memtotal_mb(&m))
            .unwrap_or(0);
        facts.env = std::env::vars().collect();
        facts.uid = std::fs::metadata("/proc/self")
            .map(|m| m.uid())
            .unwrap_or(u32::MAX);
        facts.user_id = ["LOGNAME", "USER"]
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
            .or_else(|| user_name(facts.uid))
            .unwrap_or_default();
        facts
    }

    /// Distribution facts from `/etc/os-release`; `/etc/debian_version`
    /// supplies the codename on Debian testing, whose os-release has none.
    pub fn from_os_release(os_release: &str, debian_version: &str) -> Facts {
        let fields: BTreeMap<&str, String> = os_release
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.trim(), v.trim().trim_matches('"').to_string()))
            .collect();
        let get = |k: &str| fields.get(k).cloned().unwrap_or_default();

        let id = get("ID");
        let distribution = match id.as_str() {
            "debian" => "Debian".to_string(),
            "ubuntu" => "Ubuntu".to_string(),
            "linuxmint" => "Linux Mint".to_string(),
            "pop" => "Pop!_OS".to_string(),
            other => capitalize(other),
        };
        let like = get("ID_LIKE");
        let debian_like = ["debian", "ubuntu", "linuxmint", "pop", "raspbian", "kali"];
        let os_family = if debian_like.contains(&id.as_str())
            || like
                .split_whitespace()
                .any(|l| l == "debian" || l == "ubuntu")
        {
            "Debian".to_string()
        } else {
            distribution.clone()
        };
        let mut release = get("VERSION_CODENAME");
        if release.is_empty() {
            release = get("UBUNTU_CODENAME");
        }
        if release.is_empty() {
            release = debian_version
                .trim()
                .split('/')
                .next()
                .filter(|r| !r.chars().next().is_some_and(|c| c.is_ascii_digit()))
                .unwrap_or_default()
                .to_string();
        }
        let version = get("VERSION_ID");
        let major = version.split('.').next().unwrap_or_default().to_string();
        Facts {
            distribution,
            distribution_release: release,
            distribution_version: version,
            distribution_major_version: major,
            os_family,
            ..Facts::default()
        }
    }

    pub fn is_debian_family(&self) -> bool {
        self.os_family == "Debian"
    }

    /// Whether this is WSL, checked as `play.sh` did: a Microsoft kernel.
    /// Unlike a `-WSL2` suffix, this also matches WSL1 (`…-Microsoft`) and
    /// custom WSL kernels (`…-microsoft-standard-WSL2+`).
    pub fn wsl2(&self) -> bool {
        self.kernel.to_ascii_lowercase().contains("microsoft")
    }

    /// The architecture as Go and Debian name it.
    pub fn go_arch(&self) -> &str {
        match self.architecture.as_str() {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            "i386" => "386",
            _ => "",
        }
    }
}

fn read_trimmed(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn memtotal_mb(meminfo: &str) -> u64 {
    meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
        .map(|kb| kb / 1024)
        .unwrap_or(0)
}

fn user_name(uid: u32) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let mut parts = line.split(':');
        let name = parts.next()?;
        let id = parts.nth(1)?.parse::<u32>().ok()?;
        (id == uid).then(|| name.to_string())
    })
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_debian() {
        let f = Facts::from_os_release(
            "PRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\nID=debian\nVERSION_ID=\"13\"\nVERSION_CODENAME=trixie\n",
            "13.1\n",
        );
        assert_eq!(f.distribution, "Debian");
        assert_eq!(f.distribution_release, "trixie");
        assert_eq!(f.distribution_major_version, "13");
        assert!(f.is_debian_family());
    }

    #[test]
    fn reads_ubuntu() {
        let f = Facts::from_os_release(
            "ID=ubuntu\nID_LIKE=debian\nVERSION_ID=\"24.04\"\nUBUNTU_CODENAME=noble\n",
            "",
        );
        assert_eq!(f.distribution, "Ubuntu");
        assert_eq!(f.distribution_release, "noble");
        assert_eq!(f.distribution_major_version, "24");
        assert_eq!(f.os_family, "Debian");
    }

    #[test]
    fn falls_back_to_debian_version_codename() {
        let f = Facts::from_os_release("ID=debian\n", "forky/sid\n");
        assert_eq!(f.distribution_release, "forky");
    }

    #[test]
    fn parses_meminfo() {
        assert_eq!(
            memtotal_mb("MemTotal:       16318412 kB\nMemFree: 1 kB\n"),
            15935
        );
    }

    #[test]
    fn detects_wsl2() {
        let f = Facts {
            kernel: "5.15.153.1-microsoft-standard-WSL2".into(),
            ..Facts::default()
        };
        assert!(f.wsl2());
        for kernel in ["4.4.0-19041-Microsoft", "6.6.36.3-microsoft-standard-WSL2+"] {
            let f = Facts {
                kernel: kernel.into(),
                ..Facts::default()
            };
            assert!(f.wsl2(), "{kernel}");
        }
        let f = Facts {
            kernel: "6.12.0-1-amd64".into(),
            ..Facts::default()
        };
        assert!(!f.wsl2());
    }
}
