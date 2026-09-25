// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `docker`: Docker Engine from the docker-ce repository, and the docker group.

use crate::configure::engine::{Plan, Task};
use crate::configure::modules::{apt, file, system};
use crate::configure::roles::{NOT_IN_DEV_CONTAINER, apt as apt_role};
use crate::configure::vars::{Profile, Vars};
use std::path::Path;

const PACKAGES: &[&str] = &[
    "docker-ce",
    "docker-ce-cli",
    "containerd.io",
    "docker-buildx-plugin",
    "docker-compose-plugin",
];

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let wanted = v.profile != Profile::DevContainer;

    plan.add(
        Task::new("docker/packages", "Install the Docker packages")
            .tags(&["docker"])
            .sudo()
            .after([apt_role::UPDATE])
            .when(wanted, NOT_IN_DEV_CONTAINER)
            .run(|ctx| async move {
                let keyrings =
                    file::directory(&ctx, Path::new("/etc/apt/keyrings"), Some(0o755), true)
                        .await?;
                let names: Vec<String> = PACKAGES.iter().map(|s| s.to_string()).collect();
                let hour = Some(std::time::Duration::from_secs(3600));
                Ok(keyrings.and(apt::install(&ctx, &names, hour).await?))
            }),
    );

    plan.add(
        Task::new("docker/group", "Create the docker group")
            .tags(&["docker"])
            .sudo()
            .when(wanted, NOT_IN_DEV_CONTAINER)
            .run(|ctx| async move { system::group(&ctx, "docker").await }),
    );
}
