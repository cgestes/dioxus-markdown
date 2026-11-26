use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BodyStart {
    WholeNote,
    AfterHeading,
    AfterBlankLine,
}

/// The live title and body are slices of the original Markdown, including its line endings.
pub struct Sections {
    pub hashes: Option<Range<usize>>,
    pub title: Option<Range<usize>>,
    pub body: usize,
    pub origin: BodyStart,
}

impl Sections {
    pub fn plain() -> Self {
        Self {
            hashes: None,
            title: None,
            body: 0,
            origin: BodyStart::WholeNote,
        }
    }

    pub fn read(markdown: &str) -> Self {
        Self::with_origin(markdown, BodyStart::AfterBlankLine)
    }

    pub fn with_origin(markdown: &str, origin: BodyStart) -> Self {
        if origin == BodyStart::WholeNote {
            return Self::plain();
        }
        let newline = markdown.find('\n').unwrap_or(markdown.len());
        let line = markdown[..newline]
            .strip_suffix('\r')
            .unwrap_or(&markdown[..newline]);
        let indent = line.len() - line.trim_start_matches(' ').len();
        let marks = line[indent..].bytes().take_while(|&b| b == b'#').count();
        let tail = &line[indent + marks..];
        if indent > 3
            || !(1..=6).contains(&marks)
            || !(tail.is_empty() || tail.starts_with([' ', '\t']))
        {
            return Self::plain();
        }
        let start = indent + marks + usize::from(!tail.is_empty());
        // A closing sequence such as " ##" belongs to the heading markup.
        let trimmed = line.trim_end_matches([' ', '\t']);
        let before_hashes = trimmed.trim_end_matches('#');
        let end = if before_hashes.len() < trimmed.len()
            && before_hashes.len() >= start
            && before_hashes.ends_with([' ', '\t'])
        {
            before_hashes.trim_end_matches([' ', '\t']).len().max(start)
        } else {
            line.len()
        };
        let mut body = (newline + 1).min(markdown.len());
        // Keep one separating blank line in the Markdown between the two controls.
        let rest = &markdown[body..];
        let blank = if rest.starts_with("\r\n") {
            2
        } else {
            usize::from(rest.starts_with('\n'))
        };
        let origin = if origin == BodyStart::AfterBlankLine && blank > 0 {
            body += blank;
            BodyStart::AfterBlankLine
        } else {
            BodyStart::AfterHeading
        };
        Self {
            hashes: Some(indent..indent + marks),
            title: Some(start..end),
            body,
            origin,
        }
    }

    pub fn title<'a>(&self, md: &'a str) -> &'a str {
        self.title.clone().map_or("", |range| &md[range])
    }
}

/// What splits a note's body into fields: a heading after the first, or a
/// table. Each gets a field of its own, and prose fills the gaps between them
/// (`gap`). The blocks the handles move are another thing: see `blocks`.
#[derive(Clone, PartialEq)]
pub enum LandmarkKind {
    Heading {
        hashes: Range<usize>,
        title: Range<usize>,
    },
    Table(usize),
}

#[derive(Clone, PartialEq)]
pub struct Landmark {
    pub line: Range<usize>,
    pub kind: LandmarkKind,
}

/// The landmarks of a note, in source order.
pub fn landmarks(md: &str, tables: &[margin_core::table::Table]) -> Vec<Landmark> {
    use pulldown_cmark::{Event, Options, Parser, Tag};
    let mut result: Vec<Landmark> = tables
        .iter()
        .enumerate()
        .map(|(i, t)| Landmark {
            line: t.range.clone(),
            kind: LandmarkKind::Table(i),
        })
        .collect();
    for (event, span) in Parser::new_ext(md, Options::ENABLE_TABLES).into_offset_iter() {
        if !matches!(event, Event::Start(Tag::Heading { .. })) {
            continue;
        }
        let start = md[..span.start].rfind('\n').map_or(0, |i| i + 1);
        if start == 0 {
            continue;
        } // The first ATX heading has the notebook title field.
        let end = start
            + md[start..]
                .split('\n')
                .next()
                .unwrap_or("")
                .trim_end_matches('\r')
                .len();
        let p = Sections::read(&md[start..end]);
        if let (Some(h), Some(title)) = (p.hashes, p.title) {
            result.push(Landmark {
                line: start..end,
                kind: LandmarkKind::Heading {
                    hashes: start + h.start..start + h.end,
                    title: start + title.start..start + title.end,
                },
            });
        }
    }
    result.sort_by_key(|landmark| landmark.line.start);
    result
}

/// Keep every intervening source line, including empty ones. Only the newline
/// terminating a neighboring block is outside the prose field.
pub fn gap(md: &str, body: usize, landmarks: &[Landmark], i: usize) -> Option<Range<usize>> {
    let left = i.checked_sub(1).and_then(|i| landmarks.get(i));
    let right = landmarks.get(i);
    let mut start = left.map_or(body, |b| b.line.end);
    if left.is_some() {
        start += newline_len(&md[start..]);
    }
    let mut end = right.map_or(md.len(), |b| b.line.start);
    if right.is_some() {
        if start >= end {
            return None;
        }
        if md[..end].ends_with("\r\n") {
            end -= 2;
        } else if md[..end].ends_with('\n') {
            end -= 1;
        }
    }
    Some(start..end.max(start))
}
pub fn newline_len(text: &str) -> usize {
    if text.starts_with("\r\n") {
        2
    } else {
        usize::from(text.starts_with('\n'))
    }
}
