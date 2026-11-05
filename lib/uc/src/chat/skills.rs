// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Skills: what a model is told on top of the main skill, for one kind of
//! work, turned on with `uc agent skill`. A skill is a directory with a
//! `SKILL.md` in it (the Agent Skills layout: YAML front matter with `name`
//! and `description`, then the instructions), next to the scripts and
//! references it talks about. They are found in `.agents/skills` of the
//! working directory and the ones above it up to the repository root (the
//! project's), and in `~/.agents/skills` (everyone's); a project skill wins
//! over a global one of the same name. More than one can be on at a time:
//! the ones on go into the system prompt, in the order they were turned on.

use std::path::{Path, PathBuf};

/// Where skills sit, under a project directory or the home.
pub const DIR: &str = ".agents/skills";
/// The file a skill's directory has its instructions in.
const FILE: &str = "SKILL.md";
/// The most of one skill the model is given.
const MAX_SKILL: usize = 40_000;
/// The most of a description a list shows.
const MAX_DESCRIPTION: usize = 100;

#[derive(Debug, Clone, PartialEq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Its `SKILL.md`.
    pub path: PathBuf,
    /// From `~/.agents/skills`, not the project's.
    pub global: bool,
}

impl Skill {
    /// Where it comes from, as a list shows it.
    pub fn place(&self) -> &'static str {
        if self.global { "global" } else { "project" }
    }

    /// The description on one line, cut for a list.
    pub fn short(&self) -> String {
        let flat = self
            .description
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        match flat.char_indices().nth(MAX_DESCRIPTION) {
            Some((end, _)) => format!("{}…", &flat[..end]),
            None => flat,
        }
    }
}

/// The skills there are for `dir` (a real path), with `home`'s as the
/// global ones: the project's first, nearest first, then the global ones,
/// a name only once (the first found), sorted by name. Outside a repository
/// only `dir`'s own `.agents/skills` counts as the project's, as for
/// AGENTS.md files.
pub fn found(dir: &Path, home: Option<&Path>) -> Vec<Skill> {
    let mut places: Vec<PathBuf> = Vec::new();
    let mut in_repo = false;
    for d in dir.ancestors() {
        places.push(d.join(DIR));
        if d.join(".git").exists() {
            in_repo = true;
            break;
        }
    }
    if !in_repo {
        places.truncate(1);
    }
    let global = home.map(|h| h.join(DIR));
    places.retain(|p| Some(p) != global.as_ref());
    let mut skills: Vec<Skill> = Vec::new();
    let all = places
        .iter()
        .map(|p| (p, false))
        .chain(global.iter().map(|p| (p, true)));
    for (place, global) in all {
        for skill in read_dir(place, global) {
            if !skills.iter().any(|s| s.name == skill.name) {
                skills.push(skill);
            }
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// The skills found from the working directory, with the user's home.
pub fn here() -> Vec<Skill> {
    let dir = std::env::current_dir()
        .and_then(|d| d.canonicalize())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    found(&dir, home.as_deref())
}

/// The skills in one skills directory.
fn read_dir(place: &Path, global: bool) -> Vec<Skill> {
    let Ok(entries) = std::fs::read_dir(place) else {
        return Vec::new();
    };
    let mut skills = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path().join(FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let (front, _) = split(&text);
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let field = |key: &str| {
            front
                .as_ref()
                .and_then(|f| f[key].as_str())
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        skills.push(Skill {
            name: field("name").unwrap_or(dir_name),
            description: field("description").unwrap_or_default(),
            path,
            global,
        });
    }
    skills
}

/// A `SKILL.md`'s front matter, if it has one that reads, and the rest.
fn split(text: &str) -> (Option<serde_yaml::Value>, &str) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (None, text);
    };
    let (front, body) = match rest.find("\n---") {
        Some(end) => {
            let after = &rest[end + 4..];
            let body = after.split_once('\n').map_or("", |(_, b)| b);
            (&rest[..end], body)
        }
        None => return (None, text),
    };
    (serde_yaml::from_str(front).ok(), body)
}

/// The skill of `skills` called `name`, its case not counted.
pub fn named<'s>(skills: &'s [Skill], name: &str) -> Option<&'s Skill> {
    skills
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(name.trim()))
}

/// What `words` make of the skills on, `active`: a name turns that skill
/// on, `-name` turns it off, and `none` turns them all off. A name with no
/// skill in `skills` is an error, and then nothing changes. The lines that
/// say what changed.
pub fn change(
    active: &mut Vec<String>,
    words: &[&str],
    skills: &[Skill],
) -> Result<Vec<String>, String> {
    let mut next = active.clone();
    let mut said = Vec::new();
    for word in words
        .iter()
        .map(|w| w.trim().trim_matches(','))
        .filter(|w| !w.is_empty())
    {
        if word.eq_ignore_ascii_case("none") {
            if !next.is_empty() {
                said.push("all skills off".to_string());
            }
            next.clear();
        } else if let Some(name) = word.strip_prefix('-') {
            let before = next.len();
            next.retain(|n| !n.eq_ignore_ascii_case(name));
            if next.len() != before {
                said.push(format!("skill {name} off"));
            }
        } else {
            let name = word.strip_prefix('+').unwrap_or(word);
            let skill = named(skills, name)
                .ok_or_else(|| format!("no skill {name}; `uc agent skill` lists them"))?;
            if !next.iter().any(|n| n.eq_ignore_ascii_case(&skill.name)) {
                next.push(skill.name.clone());
                said.push(format!("skill {} on", skill.name));
            }
        }
    }
    *active = next;
    Ok(said)
}

/// Turns each of `names` on if it is off and off if it is on, as the
/// picker does with the lines taken; the lines that say what changed.
pub fn toggle(active: &mut Vec<String>, names: &[String]) -> Vec<String> {
    let mut said = Vec::new();
    for name in names {
        if let Some(i) = active.iter().position(|n| n == name) {
            active.remove(i);
            said.push(format!("skill {name} off"));
        } else {
            active.push(name.clone());
            said.push(format!("skill {name} on"));
        }
    }
    said
}

/// The skills, one a line, the ones on marked with `*`.
pub fn list(skills: &[Skill], active: &[String]) -> String {
    if skills.is_empty() {
        return format!("no skills: put one in {DIR}/<name>/{FILE} in the project or in ~/{DIR}\n");
    }
    skills
        .iter()
        .map(|s| {
            let mark = if active.contains(&s.name) { "*" } else { " " };
            format!("{mark} {} ({}) {}\n", s.name, s.place(), s.short()).replace(" \n", "\n")
        })
        .collect()
}

/// The skills for the model to choose from: each with where it is and its
/// whole description, the ones on marked with `*`.
pub fn catalog(skills: &[Skill], active: &[String]) -> String {
    if skills.is_empty() {
        return list(skills, active);
    }
    skills
        .iter()
        .map(|s| {
            let mark = if active.contains(&s.name) { "*" } else { " " };
            let description = s
                .description
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "{mark} {} ({}, {})\n  {description}\n",
                s.name,
                s.place(),
                s.path.display()
            )
        })
        .collect()
}

/// The names of `active` that have a skill in `skills`.
pub fn on<'a>(skills: &[Skill], active: &'a [String]) -> Vec<&'a str> {
    active
        .iter()
        .filter(|n| named(skills, n).is_some())
        .map(String::as_str)
        .collect()
}

/// What the system prompt says of the skills on: each `SKILL.md`, headed
/// with where it sits, as the files it names are next to it. Empty without
/// any.
pub fn block(skills: &[Skill], active: &[String]) -> String {
    let mut out = String::new();
    for name in active {
        let Some(skill) = named(skills, name) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&skill.path) else {
            continue;
        };
        let (_, body) = split(&text);
        let mut body = body.trim().to_string();
        if let Some((end, _)) = body.char_indices().nth(MAX_SKILL) {
            body.truncate(end);
            body.push_str("\n[cut]");
        }
        let dir = skill.path.parent().unwrap_or(&skill.path);
        out += &format!("\n\n## Skill {} ({})\n\n", skill.name, dir.display());
        if !skill.description.is_empty() {
            out += &format!("{}\n\n", skill.description.trim());
        }
        out += &body;
    }
    if out.is_empty() {
        return out;
    }
    format!(
        "\n\n# Skills the user turned on\n\n\
         Follow them for the work they are about. The files a skill names (scripts, \
         references) are in its directory, given beside its name: read them when it \
         says to.{out}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(root: &Path, name: &str, text: &str) {
        let dir = root.join(DIR).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), text).unwrap();
    }

    #[test]
    fn finds_project_skills_over_global_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let (repo, home) = (tmp.path().join("repo"), tmp.path().join("home"));
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let sub = repo.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        skill(
            &repo,
            "review",
            "---\nname: review\ndescription: Reviews code\n---\nLook hard.\n",
        );
        skill(
            &home,
            "review",
            "---\nname: review\ndescription: the global one\n---\n",
        );
        skill(&home, "deploy", "Just text, no front matter.\n");
        let found = found(&sub, Some(&home));
        let names: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["deploy", "review"]);
        assert!(found[0].global);
        assert!(!found[1].global);
        assert_eq!(found[1].description, "Reviews code");
        let block = block(&found, &["review".into()]);
        assert!(
            block.contains("Look hard.") && !block.contains("name: review"),
            "{block}"
        );
    }

    #[test]
    fn outside_a_repository_only_the_directory_s_own_count() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("a");
        std::fs::create_dir_all(&dir).unwrap();
        skill(tmp.path(), "outer", "x");
        skill(&dir, "inner", "x");
        let names: Vec<String> = found(&dir, None).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["inner"]);
    }

    #[test]
    fn changes_the_skills_on() {
        let skills: Vec<Skill> = ["a", "b"]
            .iter()
            .map(|n| Skill {
                name: n.to_string(),
                description: String::new(),
                path: PathBuf::new(),
                global: false,
            })
            .collect();
        let mut on = Vec::new();
        assert_eq!(change(&mut on, &["a", "B"], &skills).unwrap().len(), 2);
        assert_eq!(on, ["a", "b"]);
        change(&mut on, &["-a"], &skills).unwrap();
        assert_eq!(on, ["b"]);
        assert!(change(&mut on, &["a", "nope"], &skills).is_err());
        assert_eq!(on, ["b"]);
        change(&mut on, &["none"], &skills).unwrap();
        assert!(on.is_empty());
        toggle(&mut on, &["a".into(), "b".into()]);
        toggle(&mut on, &["a".into()]);
        assert_eq!(on, ["b"]);
    }
}
