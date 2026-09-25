// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Rendering of `/etc/swanctl/swanctl.conf` for a full-mesh PKI setup.

/// A mesh peer: connection name, public address and IKE identity.
pub struct Peer {
    pub name: String,
    pub ip: String,
    pub id: String,
}

/// Local host settings and credential file basenames.
pub struct Local<'a> {
    pub ip: &'a str,
    pub id: &'a str,
    pub virtual_ip: &'a str,
    pub cert: &'a str,
    pub key: &'a str,
    pub ca: &'a str,
}

/// Render swanctl.conf content with one connection per peer.
pub fn render(local: &Local, peers: &[Peer]) -> String {
    let mut parts = vec!["connections {".to_string()];
    for peer in peers {
        let conn = format!("mesh-to-{}", peer.name);
        parts.push(format!(
            "
    {conn} {{
        local_addrs  = {local_ip}
        remote_addrs = {peer_ip}
        version      = 2
        proposals    = aes256gcm16-prfsha384-ecp384
        vips         = {vip}
        local {{
            auth  = pubkey
            certs = {cert}
            id    = {local_id}
        }}
        remote {{
            auth = pubkey
            id   = {peer_id}
        }}
        children {{
            {conn}-child {{
                local_ts      = {vip}
                remote_ts     = 192.168.88.0/24
                esp_proposals = aes256gcm16-ecp384
                start_action  = start
                dpd_action    = restart
            }}
        }}
    }}",
            local_ip = local.ip,
            peer_ip = peer.ip,
            vip = local.virtual_ip,
            cert = local.cert,
            local_id = local.id,
            peer_id = peer.id,
        ));
    }
    parts.push("}\n".to_string());
    parts.push(format!(
        "
secrets {{
    host-key {{
        file = \"/etc/swanctl/private/{key}\"
    }}
}}

authorities {{
    mesh-ca {{
        cacert = \"/etc/swanctl/x509ca/{ca}\"
    }}
}}
",
        key = local.key,
        ca = local.ca,
    ));
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_one_peer() {
        let local = Local {
            ip: "1.1.1.1",
            id: "@a",
            virtual_ip: "192.168.88.10/32",
            cert: "c.pem",
            key: "k.pem",
            ca: "ca.pem",
        };
        let peers = [Peer {
            name: "b".into(),
            ip: "2.2.2.2".into(),
            id: "@b".into(),
        }];
        let out = render(&local, &peers);
        assert!(out.starts_with("connections {\n\n    mesh-to-b {\n"));
        assert!(out.contains("            mesh-to-b-child {\n"));
        assert!(out.contains("        remote_addrs = 2.2.2.2\n"));
        assert!(out.contains("    }\n}\n\n\nsecrets {\n"));
        assert!(out.ends_with("cacert = \"/etc/swanctl/x509ca/ca.pem\"\n    }\n}\n"));
    }
}
