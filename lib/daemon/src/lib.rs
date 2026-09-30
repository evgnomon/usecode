// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The usecode daemon and its tools. `usecoded` is the one binary on
//! every host: `usecoded run` is the daemon ([`daemon`]), which grows by
//! modules - today the WireGuard mesh. `uc-daemon-install` puts it on a
//! host from the control node, and `uc-net-mesh` is the mesh's own CLI.
//! Each binary is a thin front over the modules here.

#[macro_use]
pub mod error;

pub mod app;
pub mod apply;
pub mod bundle;
pub mod config;
pub mod daemon;
pub mod flags;
pub mod install;
pub mod inventory;
pub mod iptables;
pub mod keys;
pub mod net;
pub mod remote;
pub mod setup;
pub mod wg;
