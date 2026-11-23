use super::table::eol;
use margin_editor_core::note::Splice;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

pub enum Command {
    Continue,
    Backspace,
    Indent,
    Outdent,
}
pub struct Edit {
    pub changes: Vec<Splice>,
    pub caret: [usize; 2],
}

fn line_start(md: &str, pos: usize) -> usize {
    md[..pos].rfind('\n').map_or(0, |i| i + 1)
}
fn line_end(md: &str, start: usize) -> usize {
    start
        + md[start..]
            .split('\n')
            .next()
            .unwrap_or("")
            .trim_end_matches('\r')
            .len()
}
struct Prefix<'a> {
    indent: &'a str,
    mark: &'a str,
    gap: &'a str,
    task: bool,
    end: usize,
}
fn prefix(line: &str) -> Option<Prefix<'_>> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - trimmed.len()];
    let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
    let end = match trimmed.as_bytes().get(digits)? {
        b'.' | b')' if (1..=9).contains(&digits) => digits + 1,
        b'-' | b'+' | b'*' if digits == 0 => 1,
        _ => return None,
    };
    let mark = &trimmed[..end];
    let rest = &trimmed[end..];
    let tail = rest.trim_start_matches([' ', '\t']);
    let gap = &rest[..rest.len() - tail.len()];
    if gap.is_empty() && !tail.is_empty() {
        return None;
    }
    let task = ["[ ]", "[x]", "[X]"].iter().any(|p| {
        tail.strip_prefix(p)
            .is_some_and(|s| s.is_empty() || s.starts_with([' ', '\t']))
    });
    let end = line.len() - tail.len()
        + if task {
            3 + tail[3..].len() - tail[3..].trim_start_matches([' ', '\t']).len()
        } else {
            0
        };
    Some(Prefix {
        indent,
        mark,
        gap,
        task,
        end,
    })
}

struct Item {
    range: Range<usize>,
    parent: Option<usize>,
    previous: Option<usize>,
}

fn items(md: &str) -> Vec<Item> {
    let mut result = Vec::new();
    let mut stack = Vec::new();
    let mut siblings = Vec::new();
    for (e, range) in
        Parser::new_ext(md, Options::ENABLE_TASKLISTS | Options::ENABLE_TABLES).into_offset_iter()
    {
        match e {
            Event::Start(Tag::List(_)) => siblings.push(None),
            Event::End(TagEnd::List(_)) => {
                siblings.pop();
            }
            Event::Start(Tag::Item) => {
                let i = result.len();
                let previous = siblings.last_mut().and_then(|s| s.replace(i));
                result.push(Item {
                    range,
                    previous,
                    parent: stack.last().copied(),
                });
                stack.push(i);
            }
            Event::End(TagEnd::Item) => {
                stack.pop();
            }
            _ => {}
        }
    }
    result
}

pub fn edit(md: &str, [a, b]: [usize; 2], command: Command) -> Option<Edit> {
    let (lo, hi) = (a.min(b), a.max(b));
    let first = line_start(md, lo);
    let end = line_end(md, first);
    let marker = prefix(&md[first..end])?;
    if matches!(command, Command::Backspace) && (a != b || lo != first + marker.end) {
        return None;
    }
    let items = items(md);
    let item = items
        .iter()
        .find(|i| i.range.start == first + marker.indent.len())?;
    if matches!(command, Command::Backspace) {
        if item.parent.is_some() {
            return edit(md, [a, b], Command::Outdent);
        }
        return Some(Edit {
            changes: vec![Splice::new(first..lo, "")],
            caret: [first; 2],
        });
    }
    if matches!(command, Command::Continue) {
        if hi > end || lo < first + marker.end {
            return None;
        }
        if lo == hi && md[first + marker.end..end].trim().is_empty() {
            if item.parent.is_some() {
                return edit(md, [a, b], Command::Outdent);
            }
            let insert = if first > 0 { eol(md) } else { "" };
            return Some(Edit {
                changes: vec![Splice::new(first..end, insert)],
                caret: [first + insert.len(); 2],
            });
        }
        let next = if let Some(digit) = marker.mark.strip_suffix(['.', ')']) {
            format!(
                "{}{}",
                digit.parse::<u64>().ok()? + 1,
                &marker.mark[digit.len()..]
            )
        } else {
            marker.mark.to_owned()
        };
        let insert = format!(
            "{}{}{next}{}{}",
            eol(md),
            marker.indent,
            if marker.gap.is_empty() {
                " "
            } else {
                marker.gap
            },
            if marker.task { "[ ] " } else { "" }
        );
        return Some(Edit {
            caret: [lo + insert.len(); 2],
            changes: vec![Splice::new(lo..hi, insert)],
        });
    }
    let outdent = matches!(command, Command::Outdent);
    let width = if outdent {
        let parent = items.get(item.parent?)?.range.start;
        marker
            .indent
            .len()
            .checked_sub(parent - line_start(md, parent))?
    } else {
        let previous = items.get(item.previous?)?.range.start;
        let p = prefix(&md[line_start(md, previous)..line_end(md, previous)])?;
        p.mark.len() + p.gap.len().max(1)
    };
    let last = items
        .iter()
        .filter(|i| i.range.start >= lo && i.range.start < hi)
        .map(|i| i.range.end)
        .max()
        .unwrap_or(hi)
        .max(item.range.end);
    let mut changes = Vec::new();
    let mut at = first;
    for line in md[first..last].split_inclusive('\n') {
        if !line.trim().is_empty() {
            let remove = if outdent {
                width.min(line.len() - line.trim_start_matches([' ', '\t']).len())
            } else {
                0
            };
            changes.push(Splice::new(
                at..at + remove,
                if outdent {
                    String::new()
                } else {
                    " ".repeat(width)
                },
            ));
        }
        at += line.len();
    }
    let caret = [a, b].map(|pos| {
        changes
            .iter()
            .filter(|e| e.range.start <= pos)
            .fold(pos, |p, e| {
                p + e.text.len() - e.range.len().min(pos - e.range.start)
            })
    });
    Some(Edit { changes, caret })
}
