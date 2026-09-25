// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! MAC address derivation and guest IP discovery.

use std::thread;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::libvirt::{Connection, Domain};
use crate::wyhash;

/// Polls libvirt until the domain reports an address for `mac_addr`, preferring IPv4.
pub fn get_ip_address(domain: &Domain, mac_addr: &str, max_retries: u32) -> Result<String> {
    info!(
        "Attempting to get IP for {} (MAC: {mac_addr})...",
        domain.name()
    );

    for _ in 0..max_retries {
        let ips = match domain.ip_addresses(mac_addr) {
            Ok(ips) => ips,
            // No interfaces reported yet; wait and retry.
            Err(Error::Vm(_)) => {
                thread::sleep(Duration::from_secs(1));
                continue;
            }
            Err(err) => return Err(err),
        };

        if let Some(ipv4) = ips.iter().find(|ip| !ip.contains(':')) {
            return Ok(ipv4.clone());
        }
        // If no IPv4, return the first IPv6.
        if let Some(ip) = ips.into_iter().next() {
            return Ok(ip);
        }

        thread::sleep(Duration::from_secs(1));
    }

    Err(Error::vm("IP address not found"))
}

/// Derives a stable MAC in the QEMU/KVM reserved range (52:54:00:xx:xx:xx) from the domain name.
pub fn generate_mac_address(domain_name: &str) -> String {
    let hash = wyhash::hash(0, domain_name.as_bytes());
    format!(
        "52:54:00:{:02x}:{:02x}:{:02x}",
        (hash >> 16) as u8,
        (hash >> 8) as u8,
        hash as u8,
    )
}

/// Prints every running domain with the addresses libvirt knows about.
pub fn print_all_domain_ips(conn: &Connection) -> Result<()> {
    let domains = conn.list_domains(false)?;

    if domains.is_empty() {
        info!("No running domains found");
        return Ok(());
    }

    info!("Running domains:");
    for name in domains {
        let Ok(domain) = conn.lookup_domain(&name) else {
            continue;
        };
        match domain.interface_addresses() {
            Ok(addresses) if !addresses.is_empty() => {
                for addr in addresses {
                    info!("  {}: {} (MAC: {})", addr.interface, addr.ip, addr.hwaddr);
                }
            }
            _ => info!("  {name}: IP not available yet"),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::generate_mac_address;

    #[test]
    fn mac_is_stable_and_in_qemu_range() {
        let mac = generate_mac_address("myvm");
        assert_eq!(mac, generate_mac_address("myvm"));
        assert!(mac.starts_with("52:54:00:"));
        assert_eq!(mac.len(), 17);
        assert_ne!(mac, generate_mac_address("othervm"));
    }
}
