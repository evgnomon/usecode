<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# Agent guide: `uc sync`

## How a run works

`uc sync` builds one plan of tasks and runs it as a parallel graph.

* `vars.rs` loads the facts, the profile and the user config once, before
  anything runs. They are read-only afterwards.
* `roles/<role>.rs` adds the role's tasks to the plan. A task id is
  `<role>/<step>`, and its role is also its tag.
* `Task::after` sets the order (`role/*` means every task of a role).
  Anything without one is free to run in parallel.
* `Task::when` turns a task off for this machine, e.g. the profile or the
  OS. The task stays in the plan, shows as `skip` with the reason, and
  still lets the tasks after it run.
* `modules/` holds the reusable steps (`file`, `copy`, `apt`, `git`, ...).
  Each one is idempotent, reports `ok` or `changed`, and honours `--check`.

## Opt-in: uc only touches what it needs

By default a run only does what uc itself needs. That is the `ESSENTIAL`
list in `roles/mod.rs`: `~/.local/bin`, the `~/.config/usecode` link and
building uc. The `sync` list in the user config turns on everything else,
by role, task id or glob, or with `all`.

`Plan::opt_in` tags every task that isn't wanted with `never`, so it stays
in the graph but only runs when asked for by tag (`uc sync -t fonts`).

When you add something:

* A new role or task is off until a user lists it. Only add it to
  `ESSENTIAL` if the uc binary or one of its plugins can't work without it.
* If a part needs something from another role (the Alacritty config names
  the CaskaydiaMono font), put it in `REQUIRES`. Don't make it essential.
* Add the role to the `plan()` in `roles/mod.rs`. The test
  `the_configuration_plan_resolves` checks that ids are unique and that
  the dependencies form no cycle.

## Checking a change

```sh
uc sync -l                 # what this config selects, with tags and deps
uc sync -C                 # dry run
cargo test --lib configure::
```
