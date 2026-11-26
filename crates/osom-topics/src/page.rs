//! Reading a page of the space: its tables, its task items, and the display
//! text of inline Markdown.
use margin_editor_core::table;
use margin_projection::reading::{self, Block};
use pulldown_cmark::{Event, Options, Parser, Tag};
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
                heading: margin_editor_core::heading::above(md, t.range.start).map(String::from),
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

/// Task items of a page, as the notebook reads them: (text, checked).
pub fn tasks(md: &str) -> Vec<(String, bool)> {
    reading::read(md)
        .into_iter()
        .filter_map(|block| match block {
            Block::Text { passage } if passage.kind == "item" => {
                let (first, rest) = passage.runs.split_first()?;
                (first.style == "box").then(|| {
                    let text: String = rest.iter().map(|r| r.text.as_str()).collect();
                    (text.trim().to_string(), first.text.starts_with('☑'))
                })
            }
            _ => None,
        })
        .collect()
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
