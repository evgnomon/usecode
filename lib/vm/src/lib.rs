// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! vm - A lightweight KVM/QEMU virtual machine creation tool.

#[macro_use]
pub mod log;

pub mod cloudinit;
pub mod config;
pub mod error;
pub mod libvirt;
pub mod network;
pub mod ssh_conf;
pub mod vm;
pub mod wyhash;

/// Version information
pub const VERSION: &str = "0.7.0";

// Re-export commonly used types and functions
pub use config::Config;
pub use error::{Error, Result};
pub use libvirt::{Connection, Domain};
pub use vm::{MountSpec, VmSpecs};

pub use vm::{
    config_vm, create_snapshot, create_vm, delete_snapshot, delete_vm, fork_vm, get_vm_ip,
    inspect_vm, list_snapshots, list_vms, mount_vm, restart_vm, restore_snapshot, show_vm_info,
    start_vm, stop_vm,
};
