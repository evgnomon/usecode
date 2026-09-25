// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The task graph and its parallel scheduler.
//!
//! A [`Task`] plays the part of an Ansible task: it has an id, tags and a body,
//! but instead of running at a fixed position in a play it names the tasks it
//! must run [`after`](Task::after). The scheduler starts every task whose
//! dependencies are resolved — run, excluded by the tag selection, or disabled
//! by their condition — so independent tasks run at the same time, at most
//! `jobs` of them at once.

use crate::configure::ctx::{Ctx, Shared};
use crate::configure::glob_match;
use anyhow::{Result, anyhow, bail};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

/// What a task body reports back, the equivalent of Ansible's ok/changed/skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Already in the desired state.
    Ok,
    /// Something on the machine was changed.
    Changed,
    /// The body decided there was nothing to do, with the reason.
    Skipped(String),
}

impl Outcome {
    /// Folds the outcomes of several steps into one: any change wins over ok.
    pub fn and(self, other: Outcome) -> Outcome {
        match (self, other) {
            (Outcome::Changed, _) | (_, Outcome::Changed) => Outcome::Changed,
            (Outcome::Skipped(_), other) => other,
            (this, _) => this,
        }
    }

    pub fn changed(changed: bool) -> Outcome {
        if changed {
            Outcome::Changed
        } else {
            Outcome::Ok
        }
    }
}

type TaskFuture = Pin<Box<dyn Future<Output = Result<Outcome>> + Send>>;
type TaskFn = Box<dyn FnOnce(Ctx) -> TaskFuture + Send>;

/// One unit of configuration work.
pub struct Task {
    pub id: String,
    pub name: String,
    /// Tags given explicitly; the role and the id are matched as well.
    pub tags: BTreeSet<String>,
    /// Ids of the tasks this one runs after. `role/*` names every task of a
    /// role whose id starts with `role/`.
    pub after: Vec<String>,
    /// Runs something through sudo, so the password is asked before the run.
    pub sudo: bool,
    /// A failure is reported but does not block the dependents.
    pub ignore_errors: bool,
    /// Set when the task's condition is false, with the reason.
    pub disabled: Option<String>,
    run: Option<TaskFn>,
}

impl Task {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Task {
        Task {
            id: id.into(),
            name: name.into(),
            tags: BTreeSet::new(),
            after: Vec::new(),
            sudo: false,
            ignore_errors: false,
            disabled: None,
            run: None,
        }
    }

    pub fn tags(mut self, tags: &[&str]) -> Task {
        self.tags.extend(tags.iter().map(|t| t.to_string()));
        self
    }

    pub fn after<I, S>(mut self, deps: I) -> Task
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.after.extend(deps.into_iter().map(Into::into));
        self
    }

    pub fn sudo(mut self) -> Task {
        self.sudo = true;
        self
    }

    pub fn ignore_errors(mut self) -> Task {
        self.ignore_errors = true;
        self
    }

    /// Ansible's `when`: the task is kept in the plan, so it is listed and
    /// satisfies its dependents, but it does not run unless `cond` holds.
    /// The first false condition wins.
    pub fn when(mut self, cond: bool, reason: impl Into<String>) -> Task {
        if !cond && self.disabled.is_none() {
            self.disabled = Some(reason.into());
        }
        self
    }

    pub fn run<F, Fut>(mut self, body: F) -> Task
    where
        F: FnOnce(Ctx) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Outcome>> + Send + 'static,
    {
        self.run = Some(Box::new(move |ctx| Box::pin(body(ctx))));
        self
    }

    /// The role a task belongs to: its id up to the first `/`.
    pub fn role(&self) -> &str {
        self.id.split('/').next().unwrap_or(&self.id)
    }

    /// Whether `tag` names this task through an explicit tag or its id, the
    /// only ways to pull in a `never` task.
    fn tagged(&self, tag: &str) -> bool {
        glob_match(tag, &self.id) || self.tags.iter().any(|t| glob_match(tag, t))
    }

    fn matches(&self, tag: &str) -> bool {
        self.tagged(tag) || glob_match(tag, self.role())
    }
}

/// Which tasks the user asked for, with Ansible's tag semantics: `always`
/// tasks run unless skipped by name, `never` tasks only when asked for by an
/// explicit tag or their id.
#[derive(Debug, Default, Clone)]
pub struct Selection {
    pub tags: Vec<String>,
    pub skip_tags: Vec<String>,
    pub with_deps: bool,
}

impl Selection {
    pub fn includes(&self, task: &Task) -> bool {
        if self.skip_tags.iter().any(|t| task.matches(t)) {
            return false;
        }
        let never = task.tags.contains("never");
        let all = self.tags.is_empty() || self.tags.iter().any(|t| t == "all");
        if never {
            return !all && self.tags.iter().any(|t| task.tagged(t));
        }
        all || task.tags.contains("always") || self.tags.iter().any(|t| task.matches(t))
    }
}

/// Every task of a run, indexed by id, with the dependencies resolved.
#[derive(Default)]
pub struct Plan {
    tasks: Vec<Task>,
    index: HashMap<String, usize>,
    /// `deps[i]` are the indices task `i` runs after.
    deps: Vec<Vec<usize>>,
}

impl Plan {
    pub fn add(&mut self, task: Task) {
        self.tasks.push(task);
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    /// Applies a `when` to every task added from index `start` on, the way a
    /// condition on an `import_playbook` covers all of its plays.
    pub fn gate(&mut self, start: usize, cond: bool, reason: &str) {
        for task in &mut self.tasks[start..] {
            if !cond && task.disabled.is_none() {
                task.disabled = Some(reason.to_string());
            }
        }
    }

    pub fn deps(&self, i: usize) -> &[usize] {
        &self.deps[i]
    }

    /// Resolves the `after` names to indices and rejects duplicate ids,
    /// unknown dependencies and cycles.
    pub fn resolve(&mut self) -> Result<()> {
        self.index.clear();
        for (i, task) in self.tasks.iter().enumerate() {
            if self.index.insert(task.id.clone(), i).is_some() {
                bail!("task '{}' is defined twice", task.id);
            }
        }
        let mut deps = Vec::with_capacity(self.tasks.len());
        for task in &self.tasks {
            let mut resolved = BTreeSet::new();
            for name in &task.after {
                if let Some(prefix) = name.strip_suffix('*') {
                    resolved.extend(
                        self.tasks
                            .iter()
                            .enumerate()
                            .filter(|(_, t)| t.id.starts_with(prefix) && t.id != task.id)
                            .map(|(i, _)| i),
                    );
                } else {
                    let i = self.index.get(name).ok_or_else(|| {
                        anyhow!("task '{}' runs after unknown task '{name}'", task.id)
                    })?;
                    resolved.insert(*i);
                }
            }
            deps.push(resolved.into_iter().collect());
        }
        self.deps = deps;
        self.check_acyclic()
    }

    fn check_acyclic(&self) -> Result<()> {
        let n = self.tasks.len();
        let mut pending: Vec<usize> = self.deps.iter().map(Vec::len).collect();
        let dependents = self.dependents();
        let mut queue: VecDeque<usize> = (0..n).filter(|&i| pending[i] == 0).collect();
        let mut seen = 0;
        while let Some(i) = queue.pop_front() {
            seen += 1;
            for &d in &dependents[i] {
                pending[d] -= 1;
                if pending[d] == 0 {
                    queue.push_back(d);
                }
            }
        }
        if seen == n {
            return Ok(());
        }
        let stuck: Vec<&str> = (0..n)
            .filter(|&i| pending[i] > 0)
            .map(|i| self.tasks[i].id.as_str())
            .collect();
        bail!("dependency cycle between: {}", stuck.join(", "))
    }

    fn dependents(&self) -> Vec<Vec<usize>> {
        let mut dependents = vec![Vec::new(); self.tasks.len()];
        for (i, deps) in self.deps.iter().enumerate() {
            for &d in deps {
                dependents[d].push(i);
            }
        }
        dependents
    }

    /// Indices of the tasks the selection runs, dependencies included when
    /// asked for.
    pub fn select(&self, selection: &Selection) -> BTreeSet<usize> {
        let mut picked: BTreeSet<usize> = (0..self.tasks.len())
            .filter(|&i| selection.includes(&self.tasks[i]))
            .collect();
        if selection.with_deps {
            let mut stack: Vec<usize> = picked.iter().copied().collect();
            while let Some(i) = stack.pop() {
                for &d in &self.deps[i] {
                    let skipped = selection.skip_tags.iter().any(|t| self.tasks[d].matches(t));
                    if !skipped && picked.insert(d) {
                        stack.push(d);
                    }
                }
            }
        }
        picked
    }
}

/// Where each task ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Waiting,
    Running,
    Done(Outcome),
    Failed(String),
    /// Failed with `ignore_errors` set.
    Ignored(String),
    /// Not run because a dependency failed or the run was aborted.
    Blocked(String),
    /// Left out by the tag selection.
    Excluded,
    /// Its `when` condition was false.
    Disabled(String),
}

impl State {
    fn resolved(&self) -> bool {
        !matches!(self, State::Waiting | State::Running)
    }

    /// Whether dependents may run after this state.
    fn satisfies(&self) -> bool {
        !matches!(self, State::Failed(_) | State::Blocked(_))
    }
}

/// Counts for the closing recap.
#[derive(Debug, Default, Clone)]
pub struct Summary {
    pub ok: usize,
    pub changed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub ignored: usize,
    pub blocked: usize,
    pub elapsed: Duration,
}

impl Summary {
    pub fn success(&self) -> bool {
        self.failed == 0 && self.blocked == 0
    }
}

pub struct RunOptions {
    pub jobs: usize,
    pub fail_fast: bool,
}

/// Runs the selected tasks of `plan`, each as soon as its dependencies are
/// resolved, keeping at most `jobs` running.
pub async fn run(
    mut plan: Plan,
    selected: &BTreeSet<usize>,
    shared: Arc<Shared>,
    opts: RunOptions,
) -> Summary {
    let started = Instant::now();
    let n = plan.tasks.len();
    let dependents = plan.dependents();
    let mut state: Vec<State> = (0..n)
        .map(|i| match &plan.tasks[i].disabled {
            Some(reason) if selected.contains(&i) => State::Disabled(reason.clone()),
            _ if !selected.contains(&i) => State::Excluded,
            _ => State::Waiting,
        })
        .collect();
    let mut pending: Vec<usize> = plan.deps.iter().map(Vec::len).collect();
    let mut ready: VecDeque<usize> = (0..n)
        .filter(|&i| pending[i] == 0 && state[i] == State::Waiting)
        .collect();
    let mut summary = Summary::default();

    // Resolved tasks release their dependents; a released task that is
    // already resolved (excluded or disabled) releases its own in turn.
    let mut release: VecDeque<usize> = (0..n).filter(|&i| state[i].resolved()).collect();
    for (task, s) in plan.tasks.iter().zip(&state) {
        if let State::Disabled(reason) = s {
            shared.out.skipped(&task.id, reason);
            summary.skipped += 1;
        }
    }
    let mut blocked_by: HashMap<usize, String> = HashMap::new();

    let mut running = JoinSet::new();
    let mut running_ids: HashMap<tokio::task::Id, usize> = HashMap::new();
    let mut aborted = false;

    loop {
        while let Some(i) = release.pop_front() {
            let ok = state[i].satisfies();
            for &d in &dependents[i] {
                pending[d] -= 1;
                if !ok && state[d] == State::Waiting {
                    blocked_by
                        .entry(d)
                        .or_insert_with(|| plan.tasks[i].id.clone());
                }
                if pending[d] == 0 && state[d] == State::Waiting {
                    match blocked_by.get(&d) {
                        Some(dep) => {
                            let reason = format!("dependency {dep} did not succeed");
                            shared.out.blocked(&plan.tasks[d].id, &reason);
                            summary.blocked += 1;
                            state[d] = State::Blocked(reason);
                            release.push_back(d);
                        }
                        None => ready.push_back(d),
                    }
                }
            }
        }

        while !aborted && running.len() < opts.jobs.max(1) {
            let Some(i) = ready.pop_front() else { break };
            let changed_deps = plan.deps[i]
                .iter()
                .filter(|&&d| state[d] == State::Done(Outcome::Changed))
                .map(|&d| plan.tasks[d].id.clone())
                .collect();
            let task = &mut plan.tasks[i];
            let body = task.run.take();
            let ctx = Ctx::new(shared.clone(), task.id.clone(), changed_deps);
            let (out, id, name) = (shared.clone(), task.id.clone(), task.name.clone());
            state[i] = State::Running;
            let handle = running.spawn(async move {
                let begin = Instant::now();
                let Some(body) = body else {
                    return (Ok(Outcome::Ok), begin.elapsed());
                };
                // Quick tasks only report their result; slow ones announce
                // themselves so a long build does not look like a hang.
                let mut work = body(ctx);
                let result = tokio::select! {
                    result = &mut work => result,
                    _ = tokio::time::sleep(out.out.start_delay()) => {
                        out.out.started(&id, &name);
                        work.await
                    }
                };
                (result, begin.elapsed())
            });
            running_ids.insert(handle.id(), i);
        }

        let Some(joined) = running.join_next_with_id().await else {
            break;
        };
        let (i, result, took) = match joined {
            Ok((id, (result, took))) => (running_ids[&id], result, took),
            Err(err) => (
                running_ids[&err.id()],
                Err(anyhow!("task panicked: {err}")),
                Duration::ZERO,
            ),
        };
        let task = &plan.tasks[i];
        state[i] = match result {
            Ok(outcome) => {
                match &outcome {
                    Outcome::Ok => summary.ok += 1,
                    Outcome::Changed => summary.changed += 1,
                    Outcome::Skipped(_) => summary.skipped += 1,
                }
                shared.out.finished(&task.id, &outcome, took);
                State::Done(outcome)
            }
            Err(err) => {
                let msg = format!("{err:#}");
                shared.out.failed(&task.id, &msg, took, task.ignore_errors);
                if task.ignore_errors {
                    summary.ignored += 1;
                    State::Ignored(msg)
                } else {
                    summary.failed += 1;
                    if opts.fail_fast && !aborted {
                        aborted = true;
                        shared
                            .out
                            .note("run", "stopping after the first failure (--fail-fast)");
                    }
                    State::Failed(msg)
                }
            }
        };
        release.push_back(i);
    }

    // With --fail-fast the loop ends with tasks still waiting.
    for (i, s) in state.iter_mut().enumerate() {
        if *s == State::Waiting {
            *s = State::Blocked("run aborted".to_string());
            shared.out.blocked(&plan.tasks[i].id, "run aborted");
            summary.blocked += 1;
        }
    }
    summary.elapsed = started.elapsed();
    summary
}

/// Every tag in the plan with the number of tasks carrying it, for `--list-tags`.
pub fn tag_counts(plan: &Plan) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for task in plan.tasks() {
        let role = std::iter::once(task.role().to_string());
        let tags: BTreeSet<String> = task.tags.iter().cloned().chain(role).collect();
        for tag in tags {
            *counts.entry(tag).or_insert(0) += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure::ctx::tests::shared;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    type Log = Arc<Mutex<Vec<String>>>;

    fn logging(id: &str, log: &Log, outcome: Result<Outcome>) -> Task {
        let log = log.clone();
        let name = id.to_string();
        Task::new(id, id).run(move |_| async move {
            log.lock().unwrap().push(name);
            outcome
        })
    }

    fn plan(tasks: Vec<Task>) -> Plan {
        let mut plan = Plan::default();
        for t in tasks {
            plan.add(t);
        }
        plan.resolve().unwrap();
        plan
    }

    async fn run_all(plan: Plan, selection: &Selection, jobs: usize) -> Summary {
        let selected = plan.select(selection);
        let opts = RunOptions {
            jobs,
            fail_fast: false,
        };
        run(plan, &selected, shared(), opts).await
    }

    fn pos(log: &Log, id: &str) -> usize {
        log.lock().unwrap().iter().position(|x| x == id).unwrap()
    }

    #[tokio::test]
    async fn runs_dependencies_first() {
        let log = Log::default();
        let p = plan(vec![
            logging("a/c", &log, Ok(Outcome::Ok)).after(["a/b"]),
            logging("a/b", &log, Ok(Outcome::Changed)).after(["a/a"]),
            logging("a/a", &log, Ok(Outcome::Ok)),
        ]);
        let summary = run_all(p, &Selection::default(), 4).await;
        assert!(pos(&log, "a/a") < pos(&log, "a/b"));
        assert!(pos(&log, "a/b") < pos(&log, "a/c"));
        assert_eq!((summary.ok, summary.changed), (2, 1));
    }

    #[tokio::test]
    async fn excluded_dependencies_do_not_block() {
        let log = Log::default();
        let p = plan(vec![
            logging("x/dep", &log, Ok(Outcome::Ok)).tags(&["other"]),
            logging("y/task", &log, Ok(Outcome::Ok)).after(["x/dep"]),
        ]);
        let selection = Selection {
            tags: vec!["y".into()],
            ..Selection::default()
        };
        run_all(p, &selection, 2).await;
        assert_eq!(*log.lock().unwrap(), vec!["y/task".to_string()]);
    }

    #[tokio::test]
    async fn with_deps_pulls_in_dependencies() {
        let log = Log::default();
        let p = plan(vec![
            logging("x/dep", &log, Ok(Outcome::Ok)),
            logging("y/task", &log, Ok(Outcome::Ok)).after(["x/dep"]),
        ]);
        let selection = Selection {
            tags: vec!["y".into()],
            with_deps: true,
            ..Selection::default()
        };
        run_all(p, &selection, 2).await;
        assert_eq!(log.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn failure_blocks_dependents_but_not_siblings() {
        let log = Log::default();
        let p = plan(vec![
            logging("r/bad", &log, Err(anyhow!("boom"))),
            logging("r/child", &log, Ok(Outcome::Ok)).after(["r/bad"]),
            logging("r/grandchild", &log, Ok(Outcome::Ok)).after(["r/child"]),
            logging("r/sibling", &log, Ok(Outcome::Ok)),
        ]);
        let summary = run_all(p, &Selection::default(), 2).await;
        let ran = log.lock().unwrap().clone();
        assert!(ran.contains(&"r/sibling".to_string()));
        assert!(!ran.contains(&"r/child".to_string()));
        assert_eq!((summary.failed, summary.blocked), (1, 2));
    }

    #[tokio::test]
    async fn ignored_failures_release_dependents() {
        let log = Log::default();
        let p = plan(vec![
            logging("r/bad", &log, Err(anyhow!("boom"))).ignore_errors(),
            logging("r/child", &log, Ok(Outcome::Ok)).after(["r/bad"]),
        ]);
        let summary = run_all(p, &Selection::default(), 2).await;
        assert!(log.lock().unwrap().contains(&"r/child".to_string()));
        assert!(summary.success());
    }

    #[tokio::test]
    async fn disabled_tasks_satisfy_dependents() {
        let log = Log::default();
        let p = plan(vec![
            logging("r/off", &log, Ok(Outcome::Ok)).when(false, "not here"),
            logging("r/on", &log, Ok(Outcome::Ok)).after(["r/off"]),
        ]);
        run_all(p, &Selection::default(), 2).await;
        assert_eq!(*log.lock().unwrap(), vec!["r/on".to_string()]);
    }

    #[tokio::test]
    async fn never_more_than_jobs_at_once() {
        let now = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let tasks = (0..12)
            .map(|i| {
                let (now, peak) = (now.clone(), peak.clone());
                Task::new(format!("p/{i}"), "sleep").run(move |_| async move {
                    let current = now.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(current, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    now.fetch_sub(1, Ordering::SeqCst);
                    Ok(Outcome::Ok)
                })
            })
            .collect();
        run_all(plan(tasks), &Selection::default(), 3).await;
        assert_eq!(peak.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn dependents_see_which_dependencies_changed() {
        let seen = Arc::new(Mutex::new(false));
        let s = seen.clone();
        let p = plan(vec![
            Task::new("h/conf", "conf").run(|_| async { Ok(Outcome::Changed) }),
            Task::new("h/other", "other").run(|_| async { Ok(Outcome::Ok) }),
            Task::new("h/handler", "handler")
                .after(["h/conf", "h/other"])
                .run(move |ctx| async move {
                    *s.lock().unwrap() = ctx.deps_changed();
                    Ok(Outcome::Ok)
                }),
        ]);
        run_all(p, &Selection::default(), 2).await;
        assert!(*seen.lock().unwrap());
    }

    #[test]
    fn rejects_cycles_and_unknown_dependencies() {
        let mut p = Plan::default();
        p.add(Task::new("a/1", "").after(["a/2"]));
        p.add(Task::new("a/2", "").after(["a/1"]));
        assert!(p.resolve().unwrap_err().to_string().contains("cycle"));

        let mut p = Plan::default();
        p.add(Task::new("a/1", "").after(["nope"]));
        assert!(p.resolve().is_err());
    }

    #[test]
    fn prefix_dependencies_expand() {
        let mut p = Plan::default();
        p.add(Task::new("pkg/a", ""));
        p.add(Task::new("pkg/b", ""));
        p.add(Task::new("fonts/x", "").after(["pkg/*"]));
        p.resolve().unwrap();
        assert_eq!(p.deps(2), &[0, 1]);
    }

    #[test]
    fn tag_selection_follows_ansible() {
        let always = Task::new("apt/proxy", "").tags(&["always"]);
        let never = Task::new("apt/upgrade", "").tags(&["never", "upgrade", "apt"]);
        let plain = Task::new("git/clone:x", "").tags(&["git"]);

        let none = Selection::default();
        assert!(none.includes(&always) && none.includes(&plain) && !none.includes(&never));

        let git = Selection {
            tags: vec!["git".into()],
            ..Selection::default()
        };
        assert!(git.includes(&always) && git.includes(&plain) && !git.includes(&never));

        let upgrade = Selection {
            tags: vec!["upgrade".into()],
            ..Selection::default()
        };
        assert!(upgrade.includes(&never));

        // The role name alone does not pull in a `never` task that lacks it.
        let autoremove = Task::new("apt/autoremove", "").tags(&["never", "autoremove"]);
        let apt = Selection {
            tags: vec!["apt".into()],
            ..Selection::default()
        };
        assert!(!apt.includes(&autoremove));

        let by_id = Selection {
            tags: vec!["git/clone:*".into()],
            ..Selection::default()
        };
        assert!(by_id.includes(&plain));

        let skip = Selection {
            skip_tags: vec!["always".into()],
            ..Selection::default()
        };
        assert!(!skip.includes(&always));
    }
}
