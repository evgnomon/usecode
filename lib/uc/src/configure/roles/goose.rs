// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `goose`: the Goose CLI configuration and recipes.

use crate::configure::engine::{Plan, Task};
use crate::configure::modules::{copy, file};
use crate::configure::vars::Vars;

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let template = v.role("goose").join("templates/config.yaml.j2");
    plan.add(
        Task::new("goose/config", "Generate the Goose configuration")
            .tags(&["goose"])
            .run(move |ctx| async move {
                let home = &ctx.vars().home;
                let config = copy::template(
                    &ctx,
                    &template,
                    &home.join(".config/goose/config.yaml"),
                    None,
                    false,
                )
                .await?;
                let recipes = file::link(
                    &ctx,
                    &home.join("src/github.com/evgnomon/agents/goose/recipes"),
                    &home.join(".config/goose/recipes"),
                    false,
                )
                .await?;
                Ok(config.and(recipes))
            }),
    );
}
