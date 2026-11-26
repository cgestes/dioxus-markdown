//! Reading a page of the space: its tables, its task items, and the display
//! text of inline Markdown.
use margin_core::table;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;

/// A Markdown table with the heading it sits under.
#[derive(Clone, Debug, PartialEq)]
pub struct MdTable {
    pub heading: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl MdTable {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|h| h.eq_ignore_ascii_case(name))
    }

    /// The first column whose header starts with `prefix`, ignoring case.
    pub fn column_starting(&self, prefix: &str) -> Option<usize> {
        self.headers
            .iter()
            .position(|h| h.to_lowercase().starts_with(&prefix.to_lowercase()))
    }

    pub(crate) fn get(&self, row: &[String], name: &str) -> String {
        self.column(name)
            .and_then(|c| row.get(c))
            .map(|s| plain(s))
            .unwrap_or_default()
    }
}

pub fn md_tables(md: &str) -> Vec<MdTable> {
    table::tables(md)
        .into_iter()
        .map(|t| {
            let mut rows = t.texts(md).into_iter();
            MdTable {
                heading: margin_core::heading::above(md, t.range.start).map(String::from),
                headers: rows.next().unwrap_or_default(),
                rows: rows.collect(),
            }
        })
        .collect()
}

/// Whether a table cell says nothing: empty, a dash, or `_TBD_`.
pub fn blank(s: &str) -> bool {
    let t = s.trim().trim_matches('_');
    t.is_empty() || t.chars().all(|c| !c.is_alphanumeric()) || t.eq_ignore_ascii_case("tbd")
}

/// One line of inline Markdown, read by pulldown-cmark.
#[derive(Default)]
pub(crate) struct Inline {
    pub(crate) text: String,
    pub(crate) code: Vec<String>,
    pub(crate) links: Vec<String>,
}

pub(crate) fn inline(s: &str) -> Inline {
    let mut out = Inline::default();
    // A leading letter keeps the line a paragraph, so `- x` or `1. x` stay literal.
    let source = format!("x{s}");
    for event in Parser::new_ext(&source, Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Text(t) => out.text.push_str(&t),
            Event::Code(c) => {
                out.text.push_str(&c);
                out.code.push(c.to_string());
            }
            Event::Start(Tag::Link { dest_url, .. }) => out.links.push(dest_url.to_string()),
            Event::SoftBreak | Event::HardBreak => out.text.push(' '),
            _ => {}
        }
    }
    out.text = out.text.strip_prefix('x').unwrap_or(&out.text).trim().to_string();
    out
}

/// Inline Markdown to display text: links keep their label, emphasis and
/// code punctuation go away.
pub fn plain(s: &str) -> String {
    inline(s).text
}

/// Task items of a page: (text, checked). The text is the item's first line
/// as the notebook shows it, without the Markdown's marks.
pub fn tasks(md: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut task: Option<(String, bool)> = None;
    for event in Parser::new_ext(md, Options::ENABLE_TASKLISTS) {
        match event {
            Event::TaskListMarker(checked) => task = Some((String::new(), checked)),
            Event::Text(text) | Event::Code(text) => {
                if let Some((t, _)) = task.as_mut() {
                    t.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak | Event::Start(Tag::List(_)) | Event::End(TagEnd::Item | TagEnd::Paragraph) => {
                if let Some((t, checked)) = task.take() {
                    out.push((t.trim().to_string(), checked));
                }
            }
            _ => {}
        }
    }
    out
}

/// A registry README's relations: page path → relationship, from its
/// `Relationship…` and `File` columns.
pub fn relations(readme: &str, folder: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for t in md_tables(readme) {
        let (Some(file), Some(rel)) = (t.column("File"), t.column_starting("Relationship")) else {
            continue;
        };
        for row in &t.rows {
            let (Some(cell), Some(r)) = (row.get(file), row.get(rel)) else {
                continue;
            };
            if let Some(target) = inline(cell).links.into_iter().next() {
                out.insert(format!("{folder}/{target}"), plain(r));
            }
        }
    }
    out
}
