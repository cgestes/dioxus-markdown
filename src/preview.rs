//! A read-only projection for the facing-page editor. Each displayed run
//! remembers the source bytes that produced it, including escaped punctuation.
use crate::{checkbox::markdown_markers, table};
use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Run {
    pub from: usize,
    pub to: usize,
    pub text: String,
    pub style: String,
    pub literal: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Passage {
    pub from: usize,
    pub to: usize,
    pub kind: String,
    pub depth: usize,
    pub runs: Vec<Run>,
}

impl Passage {
    fn empty(range: Range<usize>, kind: &str) -> Self {
        Self {
            from: range.start,
            to: range.end,
            kind: kind.into(),
            depth: 0,
            runs: Vec::new(),
        }
    }

    /// Offset in Parley's shaped, visible text, relative to this passage.
    pub fn source_at(&self, offset: usize) -> usize {
        let mut seen = 0;
        for run in &self.runs {
            if offset < seen + run.text.len() {
                return if run.literal {
                    run.from + run.text.floor_char_boundary(offset - seen)
                } else if offset == seen {
                    run.from
                } else {
                    run.to
                };
            }
            seen += run.text.len();
        }
        self.runs.last().map_or(self.from, |r| r.to)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Block {
    Text {
        passage: Passage,
    },
    Table {
        from: usize,
        to: usize,
        align: Vec<&'static str>,
        widths: Vec<usize>,
        rows: Vec<Vec<Passage>>,
    },
}

fn line_of(lines: &[Passage], offset: usize) -> usize {
    lines
        .partition_point(|line| line.from <= offset)
        .saturating_sub(1)
}

fn push_run(line: &mut Passage, from: usize, to: usize, text: &str, style: &str, literal: bool) {
    if text.is_empty() {
        return;
    }
    if literal
        && let Some(last) = line.runs.last_mut()
        && last.literal
        && last.to == from
        && last.style == style
    {
        last.text.push_str(text);
        last.to = to;
    } else {
        line.runs.push(Run {
            from,
            to,
            text: text.into(),
            style: style.into(),
            literal,
        });
    }
}

fn text_event(lines: &mut [Passage], md: &str, range: Range<usize>, value: &str, style: &str) {
    let raw = &md[range.clone()];
    if raw.starts_with('&')
        && raw.ends_with(';')
        && !value.is_empty()
        && !raw[1..raw.len() - 1].contains(['&', ';'])
    {
        let line = line_of(lines, range.start);
        push_run(
            &mut lines[line],
            range.start,
            range.end,
            value,
            style,
            false,
        );
        return;
    }
    if let Some(at) = raw.find(value) {
        let mut start = range.start + at;
        let first = line_of(lines, start);
        for (line, part) in lines[first..].iter_mut().zip(value.split_inclusive('\n')) {
            let text = part.trim_end_matches(['\r', '\n']);
            push_run(line, start, start + text.len(), text, style, true);
            start += part.len();
        }
        return;
    }
    // A GFM cell can contain an escaped pipe inside an inline code span.
    // Split identity runs so a click after the backslash still lands exactly.
    let mut i = range.start;
    let line = line_of(lines, i);
    for ch in value.chars() {
        let mut encoded = [0; 4];
        let s = ch.encode_utf8(&mut encoded);
        if let Some(n) = md[i..range.end].find(&*s) {
            i += n;
            push_run(&mut lines[line], i, i + s.len(), s, style, true);
            i += s.len();
        } else {
            // Entity-decoded text has no interior source caret.
            push_run(&mut lines[line], i, range.end, s, style, false);
            i = range.end;
        }
    }
}

fn split_markers(line: &mut Passage, md: &str, markers: &[crate::checkbox::Marker]) {
    let first = markers.partition_point(|m| m.open < line.from);
    let markers = &markers[first..markers.partition_point(|m| m.open < line.to)];
    if markers.is_empty() {
        return;
    }
    let old = std::mem::take(&mut line.runs);
    for run in old {
        if !run.literal {
            line.runs.push(run);
            continue;
        }
        let mut at = run.from;
        for m in markers
            .iter()
            .filter(|m| m.open < run.to && m.open + 3 > run.from)
        {
            if at < m.open {
                push_run(line, at, m.open, &md[at..m.open], &run.style, true);
            }
            if m.open >= run.from {
                push_run(
                    line,
                    m.open,
                    m.open + 3,
                    if m.checked { "☑" } else { "☐" },
                    "box",
                    false,
                );
            }
            at = (m.open + 3).min(run.to);
        }
        if at < run.to {
            push_run(line, at, run.to, &md[at..run.to], &run.style, true);
        }
    }
}

fn inline_style(tags: &[&str]) -> String {
    tags.iter()
        .copied()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_highlights(line: &mut Passage, md: &str) {
    if line.kind == "pre" {
        return;
    }
    let old = std::mem::take(&mut line.runs);
    for run in old {
        if !run.literal || run.style.contains("code") || !run.text.contains("==") {
            line.runs.push(run);
            continue;
        }
        let mut at = 0usize;
        let s = run.text.as_str();
        while let Some(rel_open) = s[at..].find("==") {
            let open = at + rel_open;
            let body_start = open + 2;
            let Some(rel_close) = s[body_start..].find("==") else {
                break;
            };
            let close = body_start + rel_close;
            let body = &s[body_start..close];
            if body.is_empty() || body.starts_with(' ') || body.ends_with(' ') {
                push_run(
                    line,
                    run.from + at,
                    run.from + body_start,
                    &s[at..body_start],
                    &run.style,
                    true,
                );
                at = body_start;
                continue;
            }
            if open > at {
                push_run(line, run.from + at, run.from + open, &s[at..open], &run.style, true);
            }
            let hi_style = if run.style.is_empty() {
                "highlight".to_owned()
            } else {
                format!("{} highlight", run.style)
            };
            push_run(
                line,
                run.from + body_start,
                run.from + close,
                body,
                &hi_style,
                true,
            );
            at = close + 2;
        }
        if at < s.len() {
            push_run(line, run.from + at, run.to, &s[at..], &run.style, true);
        }
    }
    let _ = md;
}

pub fn document(md: &str) -> Vec<Block> {
    let mut at = 0;
    let mut lines: Vec<_> = md
        .split('\n')
        .map(|s| {
            let range = at..at + s.trim_end_matches('\r').len();
            at += s.len() + 1;
            Passage::empty(
                range,
                if s.trim().is_empty() {
                    "blank"
                } else {
                    "paragraph"
                },
            )
        })
        .collect();
    let mut tags = Vec::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut quote = 0;
    let markers = markdown_markers(md);
    for (event, range) in Parser::new_ext(
        md,
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH,
    )
    .into_offset_iter()
    {
        let i = line_of(&lines, range.start);
        match event {
            Event::Start(tag) => {
                tags.push(match &tag {
                    Tag::Strong => "strong",
                    Tag::Emphasis => "em",
                    Tag::Strikethrough => "strike",
                    Tag::Link { .. } => "link",
                    Tag::CodeBlock(_) => "code",
                    _ => "",
                });
                match tag {
                    Tag::Heading { level, .. } => {
                        lines[i].kind = match level {
                            HeadingLevel::H1 => "h1",
                            HeadingLevel::H2 => "h2",
                            HeadingLevel::H3 => "h3",
                            HeadingLevel::H4 => "h4",
                            HeadingLevel::H5 => "h5",
                            HeadingLevel::H6 => "h6",
                        }
                        .into()
                    }
                    Tag::BlockQuote(_) => quote += 1,
                    Tag::List(start) => lists.push(start),
                    Tag::Item => {
                        let line = &mut lines[i];
                        line.depth = lists.len().saturating_sub(1);
                        line.kind = "item".into();
                        let end = md[range.clone()]
                            .find(char::is_whitespace)
                            .unwrap_or(1)
                            .min(range.len());
                        let label = if let Some(Some(n)) = lists.last_mut() {
                            let label = format!("{n}. ");
                            *n += 1;
                            label
                        } else {
                            "• ".into()
                        };
                        let start = range.start;
                        let first = markers.partition_point(|m| m.open < start + end);
                        let has_task = markers.get(first).is_some_and(|m| {
                            m.open <= line.to && md[start + end..m.open].trim().is_empty()
                        });
                        if !has_task {
                            push_run(line, start, start + end, &label, "bullet", false);
                        }
                    }
                    _ => {}
                }
            }
            Event::End(tag) => {
                tags.pop();
                match tag {
                    TagEnd::List(_) => {
                        lists.pop();
                    }
                    TagEnd::BlockQuote(_) => quote -= 1,
                    _ => {}
                }
            }
            Event::TaskListMarker(checked) => push_run(
                &mut lines[i],
                range.start,
                range.end,
                if checked { "☑ " } else { "☐ " },
                "box",
                false,
            ),
            Event::Rule => lines[i].kind = "rule".into(),
            Event::Code(value) => text_event(
                &mut lines,
                md,
                range,
                &value,
                &format!("{} code", inline_style(&tags)),
            ),
            Event::Text(value) | Event::Html(value) | Event::InlineHtml(value) => {
                if quote > 0 && lines[i].kind == "paragraph" {
                    lines[i].kind = "quote".into();
                }
                if tags.contains(&"code") {
                    let last = line_of(&lines, range.end.saturating_sub(1));
                    for line in &mut lines[i..=last] {
                        line.kind = "pre".into();
                    }
                }
                text_event(&mut lines, md, range, &value, &inline_style(&tags));
            }
            _ => {}
        }
    }
    for line in &mut lines {
        split_markers(line, md, &markers);
        split_highlights(line, md);
    }
    for callout in crate::callout::parse_callouts(md) {
        let first = line_of(&lines, callout.range.start);
        let last = line_of(&lines, callout.range.end.saturating_sub(1));
        let cls = callout.kind.css_class();
        for (idx, line) in lines[first..=last].iter_mut().enumerate() {
            let mut kind = format!("callout {cls}");
            if idx == 0 {
                kind.push_str(" callout-first callout-head");
            }
            if first + idx == last {
                kind.push_str(" callout-last");
            }
            line.kind = kind;
            if idx == 0 {
                let mut runs = vec![Run {
                    from: callout.tag_range.start,
                    to: callout.tag_range.end,
                    text: callout.kind.label().into(),
                    style: format!("callout-badge {cls}"),
                    literal: false,
                }];
                if let Some(title) = &callout.title_range {
                    runs.push(Run {
                        from: title.start,
                        to: title.end,
                        text: format!(" {}", &md[title.clone()]),
                        style: "callout-title".into(),
                        literal: false,
                    });
                }
                line.runs = runs;
            }
        }
    }
    let mut tables = table::tables(md).into_iter().peekable();
    let mut output = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if tables
            .peek()
            .is_some_and(|t| t.range.start == lines[i].from)
        {
            let t = tables.next().unwrap();
            let rows: Vec<Vec<Passage>> = (0..t.rows.len())
                .map(|r| {
                    (0..t.width())
                        .map(|c| {
                            let range = t.cell(r, c);
                            let line = &lines[i + if r == 0 { 0 } else { r + 1 }];
                            let mut cell = Passage::empty(range, "cell");
                            cell.runs = line
                                .runs
                                .iter()
                                .filter(|run| run.from >= cell.from && run.to <= cell.to)
                                .cloned()
                                .collect();
                            cell
                        })
                        .collect()
                })
                .collect();
            let widths = (0..t.width())
                .map(|c| {
                    rows.iter()
                        .map(|r| {
                            r[c].runs
                                .iter()
                                .map(|run| run.text.chars().count())
                                .sum::<usize>()
                        })
                        .max()
                        .unwrap_or(1)
                        .clamp(6, 28)
                })
                .collect();
            let align = t
                .align
                .iter()
                .map(|a| match a {
                    Alignment::Right => "right",
                    Alignment::Center => "center",
                    _ => "left",
                })
                .collect();
            i += t.rows.len() + 1;
            output.push(Block::Table {
                from: t.range.start,
                to: t.range.end,
                align,
                widths,
                rows,
            });
        } else {
            let line = &lines[i];
            if !line.runs.is_empty() || line.kind != "paragraph" {
                output.push(Block::Text {
                    passage: line.clone(),
                });
            }
            i += 1;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_letters_keep_their_utf8_source_positions() {
        let md = "# Écoute\r\n\r\n**Départ** [au quai](https://example.test) [ ].\r\n\r\n| Stop | Time |\r\n| --- | ---: |\r\n| a\\|b | 🌿 |";
        let doc = document(md);
        let Block::Text { passage } = &doc[0] else {
            panic!()
        };
        assert_eq!(passage.kind, "h1");
        assert_eq!(passage.source_at("Éc".len()), md.find("oute").unwrap());
        let Block::Text { passage } = &doc[2] else {
            panic!()
        };
        let display = passage
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect::<String>();
        assert_eq!(display, "Départ au quai ☐.");
        assert_eq!(
            passage.source_at(display.find("quai").unwrap()),
            md.find("quai").unwrap()
        );
        let Block::Table { rows, align, .. } = doc.last().unwrap() else {
            panic!()
        };
        assert_eq!(align, &["left", "right"]);
        assert_eq!(
            rows[1][0]
                .runs
                .iter()
                .map(|r| r.text.as_str())
                .collect::<String>(),
            "a|b"
        );
        assert_eq!(rows[1][0].source_at(2), md.find("b |").unwrap());
    }

    #[test]
    fn tasks_nested_lists_fences_and_empty_cells_have_distinct_targets() {
        let md = "1. Outer\n   - [x] Inner\n\n```md\n# literal [ ]\n```\n\n| A | B |\n| --- | --- |\n| Only |";
        let doc = document(md);
        let Block::Text { passage: child } = &doc[1] else {
            panic!()
        };
        assert_eq!(child.depth, 1);
        assert!(child.runs[0].style.contains("box"));
        assert!(doc.iter().any(|b|matches!(b,Block::Text{passage:p} if p.kind=="pre" && p.runs.iter().any(|r|r.text.contains("# literal [ ]")))));
        let Block::Table { rows, .. } = doc.last().unwrap() else {
            panic!()
        };
        assert_eq!(rows[1][1].source_at(0), md.len());
    }
}
