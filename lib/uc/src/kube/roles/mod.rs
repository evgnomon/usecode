// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! What `uc kube configure` sets up on a cluster, one module per role, as in
//! [`configure::roles`](crate::configure::roles). Task ids are
//! `<role>/<step>`, so `-t hcloud_csi` selects a role.

pub mod forgejo;
pub mod hcloud_csi;
pub mod registry;
