// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! In-memory run tracking and the pure logic operating on it.

use serde::Serialize;

/// A tracked run. Insertion order is preserved, like the Python dict.
#[derive(Debug, Clone)]
pub struct RunInfo {
    pub run_id: String,
    pub tenant_name: String,
    pub namespace: String,
    pub job_name: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub output_dir: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct RunListItem {
    pub run_id: String,
    pub status: String,
    pub created_at: String,
    pub exit_code: Option<i32>,
}

impl From<&RunInfo> for RunListItem {
    fn from(r: &RunInfo) -> Self {
        RunListItem {
            run_id: r.run_id.clone(),
            status: r.status.clone(),
            created_at: r.created_at.clone(),
            exit_code: r.exit_code,
        }
    }
}

/// Optional filters; empty strings behave like Python falsy values (no filter).
#[derive(Debug, Default, Clone, serde::Deserialize)]
pub struct Filter {
    pub tenant_name: Option<String>,
    pub namespace: Option<String>,
    pub job_name: Option<String>,
}

fn field_matches(filter: &Option<String>, value: &str) -> bool {
    filter.as_deref().is_none_or(|f| f.is_empty() || f == value)
}

impl Filter {
    pub fn matches(&self, r: &RunInfo) -> bool {
        field_matches(&self.tenant_name, &r.tenant_name)
            && field_matches(&self.namespace, &r.namespace)
            && field_matches(&self.job_name, &r.job_name)
    }
}

/// Runs matching `pred`, newest first (stable for equal timestamps).
pub fn list_items<F: Fn(&RunInfo) -> bool>(runs: &[RunInfo], pred: F) -> Vec<RunListItem> {
    let mut items: Vec<RunListItem> = runs.iter().filter(|r| pred(r)).map(Into::into).collect();
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    items
}

/// Plan of a `/clear` operation.
#[derive(Debug, Default, PartialEq)]
pub struct ClearPlan {
    /// (run_id, output_dir) of runs to delete.
    pub deleted: Vec<(String, String)>,
    pub skipped: Vec<String>,
}

type GroupKey<'a> = (&'a str, &'a str, &'a str);

/// Select all runs except the latest of each (tenant, namespace, job) group.
/// Running runs are skipped.
pub fn plan_clear(runs: &[RunInfo], filter: &Filter) -> ClearPlan {
    let mut groups: Vec<(GroupKey, Vec<&RunInfo>)> = Vec::new();
    for r in runs.iter().filter(|r| filter.matches(r)) {
        let key = (
            r.tenant_name.as_str(),
            r.namespace.as_str(),
            r.job_name.as_str(),
        );
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(r),
            None => groups.push((key, vec![r])),
        }
    }
    let mut plan = ClearPlan::default();
    for (_, mut group) in groups {
        group.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        for r in group.into_iter().skip(1) {
            match r.status.as_str() {
                "running" => plan.skipped.push(r.run_id.clone()),
                _ => plan.deleted.push((r.run_id.clone(), r.output_dir.clone())),
            }
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: &str, job: &str, created: &str, status: &str) -> RunInfo {
        RunInfo {
            run_id: id.into(),
            tenant_name: "system".into(),
            namespace: "main".into(),
            job_name: job.into(),
            status: status.into(),
            exit_code: None,
            created_at: created.into(),
            started_at: None,
            finished_at: None,
            output_dir: format!("/d/{id}"),
            error: None,
        }
    }

    #[test]
    fn list_sorted_and_filtered() {
        let runs = vec![
            run("a", "j1", "2026-01-01T00:00:01+00:00", "completed"),
            run("b", "j2", "2026-01-01T00:00:02+00:00", "completed"),
            run("c", "j1", "2026-01-01T00:00:03+00:00", "failed"),
        ];
        let f = Filter {
            job_name: Some("j1".into()),
            namespace: Some(String::new()),
            ..Default::default()
        };
        let ids: Vec<_> = list_items(&runs, |r| f.matches(r))
            .into_iter()
            .map(|i| i.run_id)
            .collect();
        assert_eq!(ids, vec!["c", "a"]);
    }

    #[test]
    fn clear_keeps_latest_and_skips_running() {
        let runs = vec![
            run("a", "j1", "2026-01-01T00:00:01+00:00", "completed"),
            run("b", "j1", "2026-01-01T00:00:02+00:00", "running"),
            run("c", "j1", "2026-01-01T00:00:03+00:00", "failed"),
            run("d", "j2", "2026-01-01T00:00:04+00:00", "completed"),
        ];
        let plan = plan_clear(&runs, &Filter::default());
        assert_eq!(plan.deleted, vec![("a".to_string(), "/d/a".to_string())]);
        assert_eq!(plan.skipped, vec!["b".to_string()]);
    }
}
