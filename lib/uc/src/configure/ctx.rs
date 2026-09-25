// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! What a running task can reach: the variables, the template engine, the
//! output, shared locks and the outcome of its dependencies.

use crate::configure::modules::command::Cmd;
use crate::configure::report::Reporter;
use crate::configure::vars::Vars;
use anyhow::{Context, Result};
use minijinja::Environment;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::OwnedMutexGuard;

/// State shared by every task of a run.
pub struct Shared {
    pub vars: Arc<Vars>,
    /// `--check`: report what would change without changing it.
    pub check: bool,
    pub out: Reporter,
    /// Whether uc-configure itself runs as root, so sudo is not needed.
    pub is_root: bool,
    templates: Environment<'static>,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl Shared {
    pub fn new(vars: Arc<Vars>, check: bool, out: Reporter, is_root: bool) -> Shared {
        Shared {
            vars,
            check,
            out,
            is_root,
            templates: template_env(),
            locks: Mutex::new(HashMap::new()),
        }
    }
}

/// A Jinja environment configured like Ansible's templar, so the role
/// templates render unchanged: blocks swallow their newline, a file's
/// trailing newline is kept, and Python methods such as `dict.get` work.
pub fn template_env() -> Environment<'static> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_keep_trailing_newline(true);
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    env
}

/// The handle a task body receives.
#[derive(Clone)]
pub struct Ctx {
    shared: Arc<Shared>,
    id: Arc<str>,
    changed_deps: Arc<[String]>,
}

impl Ctx {
    pub fn new(shared: Arc<Shared>, id: String, changed_deps: Vec<String>) -> Ctx {
        Ctx {
            shared,
            id: id.into(),
            changed_deps: changed_deps.into(),
        }
    }

    pub fn vars(&self) -> &Vars {
        &self.shared.vars
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn check(&self) -> bool {
        self.shared.check
    }

    pub fn is_root(&self) -> bool {
        self.shared.is_root
    }

    /// Whether any dependency reported a change: the equivalent of an
    /// Ansible handler being notified.
    pub fn deps_changed(&self) -> bool {
        !self.changed_deps.is_empty()
    }

    /// Output a command produced; shown with `--verbose`.
    pub fn log(&self, line: &str) {
        self.shared.out.log(&self.id, line);
    }

    /// A message always shown, like Ansible's `debug`.
    pub fn note(&self, msg: &str) {
        self.shared.out.note(&self.id, msg);
    }

    /// Serialises tasks that share a resource that cannot be used
    /// concurrently, such as the dpkg database.
    pub async fn lock(&self, name: &str) -> OwnedMutexGuard<()> {
        let lock = self
            .shared
            .locks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(name.to_string())
            .or_default()
            .clone();
        lock.lock_owned().await
    }

    /// Renders a Jinja string against the run's variables.
    pub fn render(&self, source: &str) -> Result<String> {
        self.render_named("<inline>", source)
    }

    pub fn render_named(&self, name: &str, source: &str) -> Result<String> {
        self.shared
            .templates
            .render_named_str(name, source, &self.shared.vars.template)
            .with_context(|| format!("rendering {name}"))
    }

    /// A program run the way Ansible's `command` module does.
    pub fn cmd(&self, program: impl Into<String>) -> Cmd {
        Cmd::new(self.clone(), program.into())
    }

    /// A script run through bash, like Ansible's `shell` module.
    pub fn shell(&self, script: impl Into<String>) -> Cmd {
        self.cmd("bash").arg("-c").arg(script.into())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::configure::report::Reporter;
    use crate::configure::vars::Vars;

    /// A quiet run context for unit tests.
    pub fn shared() -> Arc<Shared> {
        shared_with(false)
    }

    pub fn shared_with(check: bool) -> Arc<Shared> {
        Arc::new(Shared::new(
            Arc::new(Vars::for_tests()),
            check,
            Reporter::silent(),
            false,
        ))
    }

    pub fn ctx(check: bool) -> Ctx {
        Ctx::new(shared_with(check), "test/task".into(), Vec::new())
    }
}
