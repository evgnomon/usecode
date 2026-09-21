//! The few bits of address parsing portman needs, in the shapes Go's
//! `net` package offered them: a CIDR splits into an address and a
//! prefix length, and a `host:port` splits into its two halves with
//! bracketed IPv6 literals understood.

use std::net::{IpAddr, Ipv4Addr};

use crate::error::{Error, Result};

/// ParseCIDR: "10.10.0.2/24" -> (10.10.0.2, 24). The address is
/// returned as written, not masked down to the network.
pub fn parse_cidr(s: &str) -> Result<(IpAddr, u8)> {
    let (addr, bits) = s
        .split_once('/')
        .ok_or_else(|| Error(format!("invalid CIDR address: {s}")))?;
    let ip: IpAddr = addr
        .parse()
        .map_err(|_| Error(format!("invalid CIDR address: {s}")))?;
    let bits: u8 = bits
        .parse()
        .map_err(|_| Error(format!("invalid CIDR address: {s}")))?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    if bits > max {
        return Err(Error(format!("invalid CIDR address: {s}")));
    }
    Ok((ip, bits))
}

/// SplitHostPort: "0.0.0.0:443" -> ("0.0.0.0", "443"), and
/// "[2001:db8::1]:443" -> ("2001:db8::1", "443"). The port is not
/// resolved, only separated, exactly as the callers need it.
pub fn split_host_port(s: &str) -> Result<(String, String)> {
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (host, rest) = rest
            .split_once(']')
            .ok_or_else(|| Error(format!("address {s}: missing ']' in address")))?;
        let port = rest
            .strip_prefix(':')
            .ok_or_else(|| Error(format!("address {s}: missing port in address")))?;
        (host.to_string(), port.to_string())
    } else {
        let (host, port) = s
            .rsplit_once(':')
            .ok_or_else(|| Error(format!("address {s}: missing port in address")))?;
        if host.contains(':') {
            return Err(Error(format!("address {s}: too many colons in address")));
        }
        (host.to_string(), port.to_string())
    };
    if port.contains(':') {
        return Err(Error(format!("address {s}: too many colons in address")));
    }
    Ok((host, port))
}

/// JoinHostPort, bracketing an IPv6 literal the way a URL or an
/// iptables `--to-destination` needs it.
pub fn join_host_port(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

/// An IPv4 network, the shape `portman add` allocates out of.
#[derive(Clone, Copy, Debug)]
pub struct Prefix {
    base: u32,
    bits: u8,
}

impl Prefix {
    /// Parse a CIDR and mask it down to its network address.
    pub fn parse(s: &str) -> Result<Prefix> {
        let (ip, bits) = parse_cidr(s)?;
        let v4 = match ip {
            IpAddr::V4(v4) => v4,
            IpAddr::V6(_) => return Err(Error(format!("{s} is not IPv4"))),
        };
        let mask = if bits == 0 {
            0
        } else {
            u32::MAX << (32 - bits)
        };
        Ok(Prefix {
            base: u32::from(v4) & mask,
            bits,
        })
    }

    pub fn bits(&self) -> u8 {
        self.bits
    }

    pub fn contains(&self, addr: Ipv4Addr) -> bool {
        let mask = if self.bits == 0 {
            0
        } else {
            u32::MAX << (32 - self.bits)
        };
        u32::from(addr) & mask == self.base
    }

    /// The nth address in the network, counting the network address as
    /// 0. Like Go's hostNumber, n below 1 is treated as 1.
    pub fn host_number(&self, n: i64) -> Ipv4Addr {
        let n = n.max(1) as u32;
        Ipv4Addr::from(self.base.wrapping_add(n))
    }

    /// Whether addr is the all-ones address of the network, which no
    /// host may use.
    pub fn is_broadcast(&self, addr: Ipv4Addr) -> bool {
        if self.bits >= 31 {
            return false;
        }
        let host_mask = u32::MAX >> self.bits;
        u32::from(addr) & host_mask == host_mask
    }
}

/// The address after addr, or None at the end of the space.
pub fn next_addr(addr: Ipv4Addr) -> Option<Ipv4Addr> {
    u32::from(addr).checked_add(1).map(Ipv4Addr::from)
}
