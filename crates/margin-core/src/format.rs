//! Inline formatting is a Markdown edit. Keep document structure and unselected Loro characters intact.
use crate::edit::Splice;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
}
impl Style {
    pub const ALL: [Self; 5] = [
        Self::Bold,
        Self::Italic,
        Self::Strike,
        Self::Code,
        Self::Link,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Bold => "Bold",
            Self::Italic => "Italic",
            Self::Strike => "Strikethrough",
            Self::Code => "Inline code",
            Self::Link => "Link",
        }
    }
}

#[derive(Clone)]
struct Mark {
    style: Style,
    outer: Range<usize>,
    inner: Range<usize>,
}
struct Piece {
    range: Range<usize>,
    marks: Vec<Mark>,
}
pub struct InlineSelection {
    pieces: Vec<Piece>,
    backwards: bool,
}
pub struct FormatEdit {
    pub changes: Vec<Splice>,
    pub selection: [usize; 2],
}

fn trimmed(md: &str, range: Range<usize>) -> Option<Range<usize>> {
    let raw = &md[range.clone()];
    let value = raw.trim();
    let start = range.start + raw.len() - raw.trim_start().len();
    (!value.is_empty()).then_some(start..start + value.len())
}

impl InlineSelection {
    pub fn read(md: &str, [a, b]: [usize; 2]) -> Option<Self> {
        let selected = a.min(b)..a.max(b);
        if selected.is_empty() || md.get(selected.clone()).is_none() {
            return None;
        }
        let mut pieces: Vec<Piece> = Vec::new();
        let mut marks: Vec<Mark> = Vec::new();
        let mut code_block = false;
        let parser = Parser::new_ext(
            md,
            Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH,
        );
        for (event, outer) in parser.into_offset_iter() {
            if outer.start >= selected.end {
                break;
            }
            let raw = &md[outer.clone()];
            let mark = match event {
                Event::Start(Tag::Strong) => Some((Style::Bold, 2)),
                Event::Start(Tag::Emphasis) => Some((Style::Italic, 1)),
                Event::Start(Tag::Strikethrough) => {
                    Some((Style::Strike, if raw.starts_with("~~") { 2 } else { 1 }))
                }
                Event::Start(Tag::Link { .. }) if raw.starts_with('[') => {
                    if let Some(close) = raw.find("](") {
                        marks.push(Mark {
                            style: Style::Link,
                            outer: outer.clone(),
                            inner: outer.start + 1..outer.start + close,
                        });
                    }
                    continue;
                }
                Event::End(
                    end_tag @ (TagEnd::Strong
                    | TagEnd::Emphasis
                    | TagEnd::Strikethrough
                    | TagEnd::Link),
                ) => {
                    let style = match end_tag {
                        TagEnd::Strong => Style::Bold,
                        TagEnd::Emphasis => Style::Italic,
                        TagEnd::Strikethrough => Style::Strike,
                        TagEnd::Link => Style::Link,
                        _ => unreachable!(),
                    };
                    if marks.last().is_some_and(|m| m.style == style) {
                        marks.pop();
                    }
                    continue;
                }
                Event::Start(Tag::CodeBlock(_)) => {
                    code_block = true;
                    continue;
                }
                Event::End(TagEnd::CodeBlock) => {
                    code_block = false;
                    continue;
                }
                Event::Code(_) => {
                    Some((Style::Code, raw.bytes().take_while(|b| *b == b'`').count()))
                }
                Event::Text(_) if !code_block => None,
                _ => continue,
            };
            let mut inner = outer.clone();
            let mut context = marks.clone();
            if let Some((style, n)) = mark {
                if n == 0 || outer.len() <= 2 * n {
                    continue;
                }
                inner = outer.start + n..outer.end - n;
                if style == Style::Code
                    && md[inner.clone()].starts_with(' ')
                    && md[inner.clone()].ends_with(' ')
                    && !md[inner.clone()].trim().is_empty()
                {
                    inner = inner.start + 1..inner.end - 1;
                }
                let mark = Mark {
                    style,
                    outer,
                    inner: inner.clone(),
                };
                if style != Style::Code {
                    marks.push(mark);
                    continue;
                }
                context.push(mark);
            }
            let range = inner.start.max(selected.start)..inner.end.min(selected.end);
            if range.is_empty() {
                continue;
            }
            if let Some(last) = pieces.last_mut()
                && last.range.end == range.start
                && context.len() == last.marks.len()
            {
                last.range.end = range.end;
            } else {
                pieces.push(Piece {
                    range,
                    marks: context,
                });
            }
        }
        for piece in &mut pieces {
            piece.range = trimmed(md, piece.range.clone()).unwrap_or(0..0);
        }
        pieces.retain(|piece| !piece.range.is_empty());
        (!pieces.is_empty()).then_some(Self {
            pieces,
            backwards: b < a,
        })
    }

    pub fn active(&self, style: Style) -> bool {
        self.pieces
            .iter()
            .all(|p| p.marks.iter().any(|m| m.style == style))
    }

    pub fn toggle(&self, md: &str, style: Style) -> FormatEdit {
        let removing = self.active(style);
        let mut changes = Vec::new();
        let mut previous = None;
        let start = self.pieces.first().unwrap().range.start;
        let end = self.pieces.last().unwrap().range.end;
        for piece in &self.pieces {
            let r = &piece.range;
            if let Some(mark) = piece.marks.iter().rev().find(|m| m.style == style) {
                if !removing || previous == Some(mark.outer.start) {
                    continue;
                }
                previous = Some(mark.outer.start);
                if style == Style::Link {
                    changes.push(Splice::new(mark.outer.start..mark.inner.start, ""));
                    changes.push(Splice::new(mark.inner.end..mark.outer.end, ""));
                    continue;
                }
                let left = trimmed(
                    md,
                    mark.inner.start..start.clamp(mark.inner.start, mark.inner.end),
                );
                let right = trimmed(
                    md,
                    end.clamp(mark.inner.start, mark.inner.end)..mark.inner.end,
                );
                let opening = &md[mark.outer.start..mark.inner.start];
                let closing = &md[mark.inner.end..mark.outer.end];
                if let Some(left) = left {
                    changes.push(Splice::new(left.end..left.end, closing));
                } else {
                    changes.push(Splice::new(mark.outer.start..mark.inner.start, ""));
                }
                if let Some(right) = right {
                    changes.push(Splice::new(right.start..right.start, opening));
                } else {
                    changes.push(Splice::new(mark.inner.end..mark.outer.end, ""));
                }
            } else {
                let mut range = r.clone();
                if let Some(code) = piece.marks.iter().find(|m| m.style == Style::Code) {
                    range = code.outer.clone();
                }
                let raw = &md[range.clone()];
                if style == Style::Link {
                    let is_url = raw.starts_with("http://") || raw.starts_with("https://");
                    let close = if is_url {
                        format!("]({raw})")
                    } else {
                        "](https://)".to_owned()
                    };
                    changes.push(Splice::new(range.start..range.start, "["));
                    changes.push(Splice::new(range.end..range.end, close));
                    continue;
                }
                let (open, close) = match style {
                    Style::Bold => ("**".into(), "**".into()),
                    Style::Italic => ("*".into(), "*".into()),
                    Style::Strike => ("~~".into(), "~~".into()),
                    Style::Code => {
                        let ticks = "`"
                            .repeat(raw.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
                        let pad = if raw.starts_with('`') || raw.ends_with('`') {
                            " "
                        } else {
                            ""
                        };
                        (format!("{ticks}{pad}"), format!("{pad}{ticks}"))
                    }
                    Style::Link => unreachable!(),
                };
                changes.push(Splice::new(range.start..range.start, open));
                changes.push(Splice::new(range.end..range.end, close));
            }
        }
        changes.sort_by_key(|edit| (edit.range.start, edit.range.end));
        if style == Style::Link && !removing && self.pieces.len() == 1 {
            let r = &self.pieces[0].range;
            let raw = &md[r.clone()];
            let is_url = raw.starts_with("http://") || raw.starts_with("https://");
            let selection = if is_url {
                [r.start + 1, r.end + 1]
            } else {
                let url_start = r.end + 3; // "[" + raw + "]("
                [url_start, url_start + "https://".len()]
            };
            return FormatEdit { changes, selection };
        }
        let mapped = |p: usize, after: bool| {
            let mut offset = 0isize;
            for e in &changes {
                if e.range.end < p || e.range.end == p && (after || !e.range.is_empty()) {
                    offset += e.text.len() as isize - e.range.len() as isize;
                }
            }
            p.saturating_add_signed(offset)
        };
        let mut selection = [mapped(start, true), mapped(end, false)];
        if self.backwards {
            selection.swap(0, 1);
        }
        FormatEdit { changes, selection }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn toggle(source: &str, s: [usize; 2], style: Style) -> (String, [usize; 2]) {
        let edit = InlineSelection::read(source, s)
            .unwrap()
            .toggle(source, style);
        let mut md = source.to_owned();
        for change in edit.changes.into_iter().rev() {
            md.replace_range(change.range, &change.text);
        }
        (md, edit.selection)
    }
    #[test]
    fn toggles_nested_styles_with_reverse_unicode_selection() {
        let mut md = "# Été 🌿".to_owned();
        let mut sel = [md.len(), 2];
        for (style, expected) in [
            (Style::Bold, "# **Été 🌿**"),
            (Style::Italic, "# ***Été 🌿***"),
            (Style::Bold, "# *Été 🌿*"),
            (Style::Italic, "# Été 🌿"),
        ] {
            (md, sel) = toggle(&md, sel, style);
            assert_eq!(md, expected);
            assert_eq!(&md[sel[1]..sel[0]], "Été 🌿");
        }
    }
    #[test]
    fn formatting_preserves_list_prefixes_table_pipes_and_code_blocks() {
        let md =
            "## Title\n\n- [ ] Café\n\n| A | B |\n| --- | ---: |\n| 🌿 | x |\n\n```\ncode\n```";
        let (value, _) = toggle(md, [0, md.len()], Style::Bold);
        assert_eq!(
            value,
            "## **Title**\n\n- [ ] **Café**\n\n| **A** | **B** |\n| --- | ---: |\n| **🌿** | **x** |\n\n```\ncode\n```"
        );
    }
    #[test]
    fn removes_style_from_only_the_selected_word() {
        let md = "**one two three**";
        let (value, s) = toggle(md, [6, 9], Style::Bold);
        assert_eq!(value, "**one** two **three**");
        assert_eq!(&value[s[0]..s[1]], "two");
        assert!(InlineSelection::read("  ", [0, 2]).is_none());
    }

    #[test]
    fn toggles_markdown_links_and_selects_url_or_label() {
        let md = "Visit canal now";
        let (linked, sel) = toggle(md, [6, 11], Style::Link);
        assert_eq!(linked, "Visit [canal](https://) now");
        assert_eq!(&linked[sel[0]..sel[1]], "https://");
        let (unlinked, sel) = toggle(&linked, [7, 12], Style::Link);
        assert_eq!(unlinked, md);
        assert_eq!(&unlinked[sel[0]..sel[1]], "canal");
        let url_md = "See https://example.com here";
        let (url_linked, sel) = toggle(url_md, [4, 23], Style::Link);
        assert_eq!(url_linked, "See [https://example.com](https://example.com) here");
        assert_eq!(&url_linked[sel[0]..sel[1]], "https://example.com");
    }
}
