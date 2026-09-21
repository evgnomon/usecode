use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::*;

/// Writes a minimal inventory to a temp dir and loads it. `host_vars`
/// maps a host name to the body of its host_vars file; every one of them
/// is also put in the portman group.
fn new_inventory(settings: &str, host_vars: &BTreeMap<&str, String>) -> (TempDir, Inventory) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("inventory");
    must_write(
        &dir.join("group_vars").join(GROUP).join("main.yml"),
        settings,
    );

    let mut hosts = String::from("all:\n  children:\n    portman:\n      hosts:\n");
    if host_vars.is_empty() {
        hosts = String::from("all:\n  children:\n    portman:\n      hosts: {}\n");
    }
    for (name, body) in host_vars {
        hosts += &format!("        {name}:\n");
        must_write(&dir.join("host_vars").join(format!("{name}.yml")), body);
    }
    must_write(&dir.join("hosts.yml"), &hosts);

    let inv = Inventory::load(&dir).expect("load");
    (tmp, inv)
}

fn must_write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

const DEFAULT_SETTINGS: &str =
    "portman_network: 10.10.0.0/24\nportman_address_start: 1\nportman_listen_port: 51820\n";

fn host_vars(address: &str) -> String {
    format!("portman_address: {address}\nportman_public_key: xxx=\n")
}

fn hosts_of(pairs: &[(&'static str, &str)]) -> BTreeMap<&'static str, String> {
    pairs.iter().map(|(n, a)| (*n, host_vars(a))).collect()
}

fn scratch_vault() -> (TempDir, Vault) {
    let tmp = tempfile::tempdir().unwrap();
    let vault = Vault {
        path: tmp.path().join("secrets.yml"),
        dir: PathBuf::new(),
        password_file: String::new(),
    };
    (tmp, vault)
}

// Allocation is the whole reason the topology is central: the next
// address is a fact about every host, so it has to be read off all of
// them at once.
#[test]
fn allocate_skips_taken_addresses() {
    let (_tmp, inv) = new_inventory(
        DEFAULT_SETTINGS,
        &hosts_of(&[
            ("edge", "10.10.0.1"),
            ("laptop", "10.10.0.2"),
            ("phone", "10.10.0.4"),
        ]),
    );

    assert_eq!(
        inv.allocate().unwrap(),
        "10.10.0.3",
        "the gap between .2 and .4"
    );
}

#[test]
fn allocate_on_an_empty_mesh_starts_at_address_start() {
    let (_tmp, inv) = new_inventory(
        "portman_network: 10.10.0.0/24\nportman_address_start: 10\n",
        &BTreeMap::new(),
    );

    assert_eq!(inv.allocate().unwrap(), "10.10.0.10");
}

// A full network must be reported, not silently wrapped around to an
// address someone already holds.
#[test]
fn allocate_reports_a_full_network() {
    // A /30 holds exactly two usable addresses: .1 and .2.
    let (_tmp, inv) = new_inventory(
        "portman_network: 10.10.0.0/30\nportman_address_start: 1\n",
        &hosts_of(&[("a", "10.10.0.1"), ("b", "10.10.0.2")]),
    );

    let err = inv.allocate().unwrap_err().to_string();
    assert!(err.contains("no free address"), "{err}");
}

// The clash this module exists to prevent: if the topology already
// contains one, adding a host must stop rather than build on top of it.
#[test]
fn allocate_refuses_an_already_clashing_topology() {
    let (_tmp, inv) = new_inventory(
        DEFAULT_SETTINGS,
        &hosts_of(&[("laptop", "10.10.0.2"), ("phone", "10.10.0.2")]),
    );

    let err = inv.allocate().unwrap_err().to_string();
    assert!(err.contains("both claim 10.10.0.2"), "{err}");
}

#[test]
fn check_address_rejects_what_allocation_would_never_hand_out() {
    let (_tmp, inv) = new_inventory(DEFAULT_SETTINGS, &hosts_of(&[("laptop", "10.10.0.2")]));

    for (address, want) in [
        ("10.10.0.7", ""),
        ("10.10.0.2", "already held by laptop"),
        ("192.168.1.5", "outside portman_network"),
        ("10.10.0.5/24", "not an IP address"),
    ] {
        match (want, inv.check_address(address)) {
            ("", Err(e)) => panic!("checkAddress({address}) = {e}, want accepted"),
            ("", Ok(())) => {}
            (want, Ok(())) => panic!("checkAddress({address}) accepted, want {want:?}"),
            (want, Err(e)) => assert!(
                e.to_string().contains(want),
                "checkAddress({address}) = {e}, want it to mention {want:?}"
            ),
        }
    }
}

// Adding a host has to leave hosts.yml usable by Ansible and by the next
// `portman add`, comments and all.
#[test]
fn add_to_group_preserves_the_file() {
    let (_tmp, inv) = new_inventory(DEFAULT_SETTINGS, &hosts_of(&[("edge", "10.10.0.1")]));

    let original = fs::read_to_string(inv.hosts_path()).unwrap();
    must_write(&inv.hosts_path(), &format!("---\n# keep me\n{original}"));

    inv.add_to_group("laptop").expect("add_to_group");

    let got = fs::read_to_string(inv.hosts_path()).unwrap();
    assert!(got.contains("# keep me"), "comments dropped:\n{got}");
    assert!(got.starts_with("---\n"), "document start dropped:\n{got}");

    must_write(&inv.host_vars_path("laptop"), &host_vars("10.10.0.2"));
    let reloaded = Inventory::load(&inv.dir).expect("reload");
    assert!(reloaded.host("laptop").is_some(), "file is:\n{got}");
    assert!(
        reloaded.host("edge").is_some(),
        "lost the host that was already there:\n{got}"
    );
}

// An empty group is written `hosts: {}`, and a group with no hosts key at
// all is legal Ansible too - both have to accept the first member.
#[test]
fn add_to_group_from_an_empty_group() {
    for (name, hosts_yaml) in [
        (
            "flow mapping",
            "all:\n  children:\n    portman:\n      hosts: {}\n",
        ),
        (
            "null hosts",
            "all:\n  children:\n    portman:\n      hosts:\n",
        ),
        ("no hosts key", "all:\n  children:\n    portman:\n"),
        ("no children", "all:\n  hosts: {}\n"),
    ] {
        let (_tmp, inv) = new_inventory(DEFAULT_SETTINGS, &BTreeMap::new());
        must_write(&inv.hosts_path(), hosts_yaml);

        inv.add_to_group("edge")
            .unwrap_or_else(|e| panic!("{name}: add_to_group: {e}"));
        must_write(&inv.host_vars_path("edge"), &host_vars("10.10.0.1"));

        let reloaded = Inventory::load(&inv.dir).unwrap_or_else(|e| panic!("{name}: reload: {e}"));
        assert!(
            reloaded.host("edge").is_some(),
            "{name}: edge is not in the group; file is:\n{}",
            fs::read_to_string(inv.hosts_path()).unwrap()
        );
    }
}

#[test]
fn endpoint_gets_the_mesh_port() {
    let (_tmp, inv) = new_inventory(DEFAULT_SETTINGS, &BTreeMap::new());

    for (input, want) in [
        ("", ""),
        ("vpn.example.com", "vpn.example.com:51820"),
        ("vpn.example.com:51821", "vpn.example.com:51821"),
        ("203.0.113.10", "203.0.113.10:51820"),
        ("[2001:db8::1]:51820", "[2001:db8::1]:51820"),
    ] {
        assert_eq!(inv.endpoint(input), want, "endpoint({input:?})");
    }
}

// A host that is already a member must not be re-added: minting a second
// keypair would break the tunnel it already has.
#[test]
fn add_refuses_an_existing_host() {
    let (_tmp, mut inv) = new_inventory(DEFAULT_SETTINGS, &hosts_of(&[("edge", "10.10.0.1")]));
    let (_vtmp, vault) = scratch_vault();

    let err = inv
        .add(
            NewHost {
                name: "edge".into(),
                ..NewHost::default()
            },
            &vault,
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("already in the mesh"), "{err}");
}

// A leftover host_vars file with no group membership is a half-added
// host; adding on top of it would orphan whatever key it names.
#[test]
fn add_refuses_an_orphaned_host_vars_file() {
    let (_tmp, mut inv) = new_inventory(DEFAULT_SETTINGS, &BTreeMap::new());
    must_write(&inv.host_vars_path("edge"), &host_vars("10.10.0.1"));
    let (_vtmp, vault) = scratch_vault();

    let err = inv
        .add(
            NewHost {
                name: "edge".into(),
                ..NewHost::default()
            },
            &vault,
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("not in the portman group"), "{err}");
}

#[test]
fn a_host_name_has_to_be_a_file_name() {
    valid_name("edge").unwrap();
    for bad in ["", "..", "a/b", "a b", "a:b"] {
        assert!(valid_name(bad).is_err(), "{bad:?} was accepted");
    }
}
