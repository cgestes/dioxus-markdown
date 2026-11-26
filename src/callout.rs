//! GFM & Obsidian-style Markdown callouts (`> [!NOTE]`, `> [!TIP]`,
//! `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`, with optional custom title).
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CalloutKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl CalloutKind {
    pub const ALL: [Self; 5] = [
        Self::Note,
        Self::Tip,
        Self::Important,
        Self::Warning,
        Self::Caution,
    ];

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag.trim().to_ascii_lowercase().as_str() {
            "note" | "info" => Some(Self::Note),
            "tip" | "hint" => Some(Self::Tip),
            "important" => Some(Self::Important),
            "warning" | "warn" => Some(Self::Warning),
            "caution" | "danger" => Some(Self::Caution),
            _ => None,
        }
    }

    pub const fn tag(self) -> &'static str {
        match self {
            Self::Note => "NOTE",
            Self::Tip => "TIP",
            Self::Important => "IMPORTANT",
            Self::Warning => "WARNING",
            Self::Caution => "CAUTION",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }

    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Note => "callout-note",
            Self::Tip => "callout-tip",
            Self::Important => "callout-important",
            Self::Warning => "callout-warning",
            Self::Caution => "callout-caution",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCallout {
    pub kind: CalloutKind,
    /// Full UTF-8 byte range of the callout blockquote (trimmed of trailing newline).
    pub range: Range<usize>,
    /// First line's UTF-8 byte range (without trailing CR/LF).
    pub header_line: Range<usize>,
    /// Exact UTF-8 byte range of `[!KIND]` on the first line.
    pub tag_range: Range<usize>,
    /// Optional custom title on the first line after `[!KIND] `.
    pub title_range: Option<Range<usize>>,
    /// UTF-8 byte range of each leading `>` marker on every line of the callout.
    pub quote_marks: Vec<Range<usize>>,
}

/// Strip one leading blockquote prefix (`0..=3` spaces + `>` + optional space/tab).
/// Returns `(gt_offset_in_line, content_start_in_line)`.
pub fn strip_quote_marker(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut spaces = 0;
    while spaces < 3 && spaces < bytes.len() && bytes[spaces] == b' ' {
        spaces += 1;
    }
    if bytes.get(spaces) != Some(&b'>') {
        return None;
    }
    let gt = spaces;
    let mut content = gt + 1;
    if matches!(bytes.get(content), Some(b' ' | b'\t')) {
        content += 1;
    }
    Some((gt, content))
}

/// Find every top-level GFM/Obsidian callout in `md` (ignoring fenced code blocks).
pub fn parse_callouts(md: &str) -> Vec<ParsedCallout> {
    let mut out = Vec::new();
    let mut quote_depth = 0usize;
    for (event, range) in Parser::new_ext(
        md,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    )
    .into_offset_iter()
    {
        match event {
            Event::Start(Tag::BlockQuote(_)) => {
                let is_top = quote_depth == 0;
                quote_depth += 1;
                if !is_top {
                    continue;
                }
                let trimmed_end = md[..range.end]
                    .trim_end_matches(['\r', '\n'])
                    .len()
                    .max(range.start);
                let block = &md[range.start..trimmed_end];
                let first_len = block.find('\n').unwrap_or(block.len());
                let first_line = block[..first_len].trim_end_matches('\r');
                let Some((_, content_off)) = strip_quote_marker(first_line) else {
                    continue;
                };
                let rest = &first_line[content_off..];
                let lead_ws = rest.len() - rest.trim_start_matches([' ', '\t']).len();
                let after_ws = &rest[lead_ws..];
                if !after_ws.starts_with("[!") {
                    continue;
                }
                let Some(close_bracket) = after_ws[2..].find(']') else {
                    continue;
                };
                let tag_name = &after_ws[2..2 + close_bracket];
                let Some(kind) = CalloutKind::from_tag(tag_name) else {
                    continue;
                };
                let tag_start = range.start + content_off + lead_ws;
                let tag_end = tag_start + 2 + close_bracket + 1;
                let after_tag = &first_line[tag_end - range.start..];
                // Require `[!TAG]` to be followed by end-of-line or whitespace (not `[!NOTE](url)`).
                if !after_tag.is_empty() && !after_tag.starts_with([' ', '\t']) {
                    continue;
                }
                let title_trim = after_tag.trim();
                let title_range = if !title_trim.is_empty() {
                    let title_lead =
                        after_tag.len() - after_tag.trim_start_matches([' ', '\t']).len();
                    let t_start = tag_end + title_lead;
                    let t_end = t_start + title_trim.len();
                    Some(t_start..t_end)
                } else {
                    None
                };

                let mut quote_marks = Vec::new();
                let mut line_off = range.start;
                for raw_line in block.split_inclusive('\n') {
                    let line = raw_line.trim_end_matches(['\r', '\n']);
                    if let Some((gt, _)) = strip_quote_marker(line) {
                        quote_marks.push(line_off + gt..line_off + gt + 1);
                    }
                    line_off += raw_line.len();
                }

                out.push(ParsedCallout {
                    kind,
                    range: range.start..trimmed_end,
                    header_line: range.start..range.start + first_line.len(),
                    tag_range: tag_start..tag_end,
                    title_range,
                    quote_marks,
                });
            }
            Event::End(pulldown_cmark::TagEnd::BlockQuote(_)) => {
                quote_depth = quote_depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_callout_kinds_with_optional_titles_and_ignores_code_fences() {
        let md = "\
> [!NOTE]
> Useful context.

> [!tip] Quick shortcut
> Press `/` on an empty line.

> [!IMPORTANT]
> Keep untouched bytes exact.

> [!WARNING]
> Check the lock gate.

> [!CAUTION]
> Irreversible action.

```markdown
> [!NOTE]
> Literal code fence, not a callout.
```

> Plain blockquote.
";
        let found = parse_callouts(md);
        assert_eq!(found.len(), 5);
        assert_eq!(
            found.iter().map(|c| c.kind).collect::<Vec<_>>(),
            vec![
                CalloutKind::Note,
                CalloutKind::Tip,
                CalloutKind::Important,
                CalloutKind::Warning,
                CalloutKind::Caution,
            ]
        );
        assert_eq!(&md[found[0].tag_range.clone()], "[!NOTE]");
        assert_eq!(found[0].title_range, None);
        assert_eq!(found[0].quote_marks.len(), 2);

        assert_eq!(&md[found[1].tag_range.clone()], "[!tip]");
        assert_eq!(
            found[1].title_range.as_ref().map(|r| &md[r.clone()]),
            Some("Quick shortcut")
        );
    }
}
