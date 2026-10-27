// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Pick lines in the terminal: type to narrow them down fuzzily, with the
//! matched letters in colour, Enter to take one (Tab to take more when
//! `multi`), Esc to take none. A small front to skim, run in-process, so uc
//! needs no fzf on the machine.
//!
//! ```no_run
//! let picked = picker::Picker::new("model> ")
//!     .query("opus")
//!     .select_one()
//!     .pick(vec![picker::Item::new("Claude Opus", "1")]);
//! ```

use std::fs::OpenOptions;
use std::sync::Arc;

use skim::prelude::*;

/// A line to pick: what is shown and matched, and what picking it gives.
#[derive(Clone, Debug)]
pub struct Item {
    pub shown: String,
    pub value: String,
}

impl Item {
    pub fn new(shown: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            shown: shown.into(),
            value: value.into(),
        }
    }

    /// A line that gives itself.
    pub fn line(line: impl Into<String>) -> Self {
        let line = line.into();
        Self::new(line.clone(), line)
    }
}

/// What was picked: the values, and the extra key (one of
/// [`Picker::key`]) it was taken with, `None` for Enter.
#[derive(Debug, Default)]
pub struct Picked {
    pub values: Vec<String>,
    pub key: Option<String>,
}

/// Shows the text for an item's value below the list.
type Preview = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// A picker, set up with the builder methods and run with [`Picker::pick`].
pub struct Picker {
    prompt: String,
    query: String,
    header: Option<String>,
    height: String,
    reverse: bool,
    multi: bool,
    select_one: bool,
    keys: Vec<String>,
    preview: Option<Preview>,
}

impl Picker {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            query: String::new(),
            header: None,
            height: "40%".into(),
            reverse: false,
            multi: false,
            select_one: false,
            keys: Vec::new(),
            preview: None,
        }
    }

    /// Starts with `query` typed.
    pub fn query(mut self, query: impl Into<String>) -> Self {
        self.query = query.into();
        self
    }

    /// A line of help above the list.
    pub fn header(mut self, header: impl Into<String>) -> Self {
        self.header = Some(header.into());
        self
    }

    /// The share of the terminal it takes, like `40%`, or `100%`.
    pub fn height(mut self, height: impl Into<String>) -> Self {
        self.height = height.into();
        self
    }

    /// The prompt on top and the list going down, not up.
    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    /// Tab picks more than one.
    pub fn multi(mut self) -> Self {
        self.multi = true;
        self
    }

    /// When the query leaves one line, it is taken straight away.
    pub fn select_one(mut self) -> Self {
        self.select_one = true;
        self
    }

    /// Another key that takes the lines, like `ctrl-r`, told apart in
    /// [`Picked::key`].
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.keys.push(key.into());
        self
    }

    /// Shows `preview(value)` of the current line below the list.
    pub fn preview(mut self, preview: impl Fn(&str) -> String + Send + Sync + 'static) -> Self {
        self.preview = Some(Arc::new(preview));
        self
    }

    /// Lets the user pick from `items`, best match first and earlier lines
    /// first among equals. `None` when they took none (Esc, Ctrl-C) or
    /// there is no terminal to ask on; see [`available`].
    pub fn pick(self, items: Vec<Item>) -> Option<Picked> {
        if !available() {
            return None;
        }
        let mut options = SkimOptionsBuilder::default();
        options
            .prompt(self.prompt)
            .query(Some(self.query))
            .header(self.header)
            .height(self.height)
            .multi(self.multi)
            .select_1(self.select_one)
            .expect(self.keys)
            .tiebreak(vec![RankCriteria::Score, RankCriteria::Index])
            .bind(vec!["ctrl-s:toggle-sort".to_string()])
            .color(Some(COLORS.to_string()));
        if self.reverse {
            options.layout("reverse".to_string());
        }
        if self.preview.is_some() {
            options
                .preview(Some(String::new()))
                .preview_window("down:60%:wrap".to_string());
        }
        let options = options.build().ok()?.build();
        let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
        for item in items {
            let _ = tx.send(Arc::new(Line {
                item,
                preview: self.preview.clone(),
            }));
        }
        drop(tx);
        let out = Skim::run_with(&options, Some(rx))?;
        if out.is_abort {
            return None;
        }
        let values: Vec<String> = out
            .selected_items
            .iter()
            .map(|item| item.output().into_owned())
            .collect();
        if values.is_empty() {
            return None;
        }
        let key = match out.final_event {
            Event::EvActAccept(Some(key)) if !key.is_empty() => Some(key),
            _ => None,
        };
        Some(Picked { values, key })
    }
}

/// fzf's colours: the matched letters green, the current line on grey with
/// its match brighter, a pink pointer and marker, a blue prompt.
const COLORS: &str = "dark,matched:108,current:254,current_bg:236,current_match:151,\
spinner:148,info:144,prompt:110,cursor:161,selected:168,header:109,border:59";

/// Whether there is a terminal to pick on.
pub fn available() -> bool {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .is_ok()
}

struct Line {
    item: Item,
    preview: Option<Preview>,
}

impl SkimItem for Line {
    fn text(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.item.shown)
    }

    fn output(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.item.value)
    }

    fn preview(&self, _: PreviewContext) -> ItemPreview {
        match &self.preview {
            Some(preview) => ItemPreview::Text(preview(&self.item.value)),
            None => ItemPreview::Global,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_gives_itself() {
        let item = Item::line("ls -la");
        assert_eq!(item.shown, "ls -la");
        assert_eq!(item.value, "ls -la");
    }

    #[test]
    fn shows_and_gives_its_value() {
        let line = Line {
            item: Item::new("Claude Opus", "3"),
            preview: Some(Arc::new(|v: &str| format!("model {v}"))),
        };
        assert_eq!(line.text(), "Claude Opus");
        assert_eq!(line.output(), "3");
    }
}
