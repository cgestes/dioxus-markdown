//! Projection between raw Markdown UTF-8 bytes and the tight display text
//! rendered in native inputs.
//!
//! - In `EditMode::Rich` (Traditional), inline delimiters stay collapsed even
//!   when the caret is inside a formatted span; checkboxes always stay visual.
//! - In `EditMode::Markdown` (Keyboard-first Live), unfocused spans render
//!   tightly with zero ghost gaps, while the span containing the cursor expands
//!   inline so its Markdown delimiters can be edited directly.
use crate::style::{
    BULLET, CALLOUT_ANY, CALLOUT_TITLE, CODE, CODE_BLOCK, DONE, HEADING, HIGHLIGHT, ITALIC, LINK,
    INLINE, PUNCTUATION, QUOTE, QUOTE_MARK, RULE, STRIKE, STRONG, callout_style, syntax_style,
};
use margin_editor_core::{
    callout::{ParsedCallout, parse_callouts, strip_quote_marker},
    checkbox::{Marker, marker_beside},
    edit::{Splice, text_change},
    syntax::highlight_code,
    table,
};
use pulldown_cmark::{CodeBlockKind, Event, LinkType, Options, Parser, Tag, TagEnd};
use std::ops::Range;

/// 2 ASCII spaces used in monospace prose (~18px slot for a 15×15 square checkbox).
pub const CHECKBOX_SLOT_MONO: &str = "  ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    Rich,
    Markdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PieceKind {
    Text,
    Hidden,
    /// Hidden block-opening syntax (e.g. a code fence header in Rich mode) where
    /// a caret at the seam belongs inside the content that follows.
    HiddenPrefix,
    Checkbox(usize),
    EscapedPipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub source: Range<usize>,
    pub display: Range<usize>,
    pub kind: PieceKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayToken {
    pub display: Range<usize>,
    pub source_start: usize,
    pub style: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayCheckbox {
    pub open: usize,
    pub checked: bool,
    pub display: Range<usize>,
    pub revealed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DelimPair {
    outer: Range<usize>,
    open: Range<usize>,
    close: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projection {
    pub field_start: usize,
    pub display: String,
    pub pieces: Vec<Piece>,
    pub tokens: Vec<DisplayToken>,
    pub checkboxes: Vec<DisplayCheckbox>,
    hidden_pairs: Vec<DelimPair>,
    pub has_hidden_heading_prefix: bool,
    pub is_cell: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawToken {
    range: Range<usize>,
    style: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CollapseGroup {
    outer: Range<usize>,
    open: Range<usize>,
    close: Option<Range<usize>>,
    is_heading_prefix: bool,
    rich_only: bool,
}

impl CollapseGroup {
    fn inline(outer: Range<usize>, open: Range<usize>, close: Option<Range<usize>>) -> Self {
        Self {
            outer,
            open,
            close,
            is_heading_prefix: false,
            rich_only: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedLink {
    pub outer: Range<usize>,
    pub label_range: Range<usize>,
    pub url: String,
    pub title: String,
    pub is_autolink: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCodeBlock {
    pub range: Range<usize>,
    pub lang: String,
    pub lang_range: Range<usize>,
    pub body_range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedMarkdown {
    groups: Vec<CollapseGroup>,
    tokens: Vec<RawToken>,
    pub links: Vec<ParsedLink>,
    pub callouts: Vec<ParsedCallout>,
    pub code_blocks: Vec<ParsedCodeBlock>,
}

impl ParsedMarkdown {
    pub fn inline_token_starts(&self, mask: u32) -> impl Iterator<Item = usize> + '_ {
        self.tokens
            .iter()
            .filter(move |t| t.style & mask != 0)
            .map(|t| t.range.start)
    }
}

fn find_bare_urls(text: &str, base: usize) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let next = [rest.find("https://"), rest.find("http://")]
            .into_iter()
            .flatten()
            .min();
        let Some(rel) = next else {
            break;
        };
        let start = at + rel;
        if start > 0
            && text[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || matches!(c, '(' | '[' | '<'))
        {
            at = start + 4;
            continue;
        }
        let tail = &text[start..];
        let mut end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '\''))
            .unwrap_or(tail.len());
        while end > 0 {
            let last = tail[..end].chars().next_back().unwrap();
            if matches!(last, '.' | ',' | ';' | ':' | '!' | '?') {
                end -= last.len_utf8();
            } else if last == ')'
                && tail[..end].bytes().filter(|&b| b == b')').count()
                    > tail[..end].bytes().filter(|&b| b == b'(').count()
            {
                end -= 1;
            } else {
                break;
            }
        }
        let scheme_len = if tail.starts_with("https://") { 8 } else { 7 };
        if end > scheme_len + 2 && tail[scheme_len..end].contains('.') {
            out.push(base + start..base + start + end);
        }
        at = start + end.max(scheme_len);
    }
    out
}

fn find_highlights(text: &str, base: usize) -> Vec<(Range<usize>, Range<usize>, Range<usize>)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(rel_open) = text[at..].find("==") {
        let open = at + rel_open;
        let inner_start = open + 2;
        let Some(rel_close) = text[inner_start..].find("==") else {
            break;
        };
        let inner_end = inner_start + rel_close;
        let body = &text[inner_start..inner_end];
        if !body.is_empty()
            && !body.contains('\n')
            && !body.starts_with(' ')
            && !body.ends_with(' ')
        {
            out.push((
                base + open..base + inner_end + 2,
                base + open..base + inner_start,
                base + inner_end..base + inner_end + 2,
            ));
            at = inner_end + 2;
        } else {
            at = inner_start;
        }
    }
    out
}

struct ActiveLink {
    outer: Range<usize>,
    open_len: usize,
    last_child: usize,
    meta: Option<(String, String, bool)>,
}

pub fn parse_markdown(md: &str) -> ParsedMarkdown {
    let callouts = parse_callouts(md);
    let mut groups = Vec::new();
    let mut tokens = Vec::new();
    let mut parsed_links = Vec::new();
    let mut code_blocks = Vec::new();
    let mut links: Vec<ActiveLink> = Vec::new();
    let mut stack: Vec<(u32, Option<(Range<usize>, Range<usize>, Range<usize>)>)> = vec![(0, None)];

    for (event, mut range) in Parser::new_ext(
        md,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    )
    .into_offset_iter()
    {
        let inherited = stack.last().unwrap().0;
        for active in &mut links {
            if range != active.outer
                && range.start >= active.outer.start
                && range.end <= active.outer.end
            {
                active.last_child = active.last_child.max(range.end);
            }
        }
        let style = match event {
            Event::Start(tag) => {
                let mut add = match tag {
                    Tag::Strong => STRONG,
                    Tag::Emphasis => ITALIC,
                    Tag::Strikethrough => STRIKE,
                    Tag::Link { .. } | Tag::Image { .. } => LINK,
                    Tag::Heading { .. } => HEADING,
                    Tag::BlockQuote(_) => {
                        if let Some(c) = callouts.iter().find(|c| c.range.start == range.start) {
                            callout_style(c.kind)
                        } else {
                            QUOTE
                        }
                    }
                    Tag::CodeBlock(_) => CODE_BLOCK,
                    _ => 0,
                };
                let count = match tag {
                    Tag::Emphasis => 1,
                    Tag::Strong => 2,
                    Tag::Strikethrough => {
                        if md[range.clone()].starts_with("~~") {
                            2
                        } else {
                            1
                        }
                    }
                    _ => 0,
                };
                let raw = &md[range.clone()];
                match &tag {
                    Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        ..
                    } => {
                        let opening = if raw.starts_with(['[', '<']) { 1 } else { 0 };
                        if opening > 0 {
                            tokens.push(RawToken {
                                range: range.start..range.start + opening,
                                style: PUNCTUATION,
                            });
                        }
                        let is_autolink =
                            matches!(link_type, LinkType::Autolink | LinkType::Email);
                        links.push(ActiveLink {
                            outer: range.clone(),
                            open_len: opening,
                            last_child: range.start + opening,
                            meta: Some((dest_url.to_string(), title.to_string(), is_autolink)),
                        });
                    }
                    Tag::Image { .. } => {
                        let opening = if raw.starts_with("![") {
                            2
                        } else if raw.starts_with(['[', '<']) {
                            1
                        } else {
                            0
                        };
                        if opening > 0 {
                            tokens.push(RawToken {
                                range: range.start..range.start + opening,
                                style: PUNCTUATION,
                            });
                            links.push(ActiveLink {
                                outer: range.clone(),
                                open_len: opening,
                                last_child: range.start + opening,
                                meta: None,
                            });
                        }
                    }
                    _ => {}
                }
                if matches!(tag, Tag::Item) && raw.starts_with(['-', '*', '+']) {
                    let trimmed = raw[1..].trim_start_matches([' ', '\t']);
                    if trimmed.starts_with("[x]") || trimmed.starts_with("[X]") {
                        add |= DONE;
                    }
                    tokens.push(RawToken {
                        range: range.start..range.start + 1,
                        style: BULLET,
                    });
                }
                if matches!(tag, Tag::BlockQuote(_)) && raw.starts_with('>') {
                    if let Some(c) = callouts.iter().find(|c| c.range.start == range.start) {
                        let cbit = callout_style(c.kind);
                        for mark in &c.quote_marks {
                            tokens.push(RawToken {
                                range: mark.clone(),
                                style: QUOTE_MARK | cbit,
                            });
                        }
                    } else if inherited & (QUOTE | CALLOUT_ANY) == 0 {
                        let mut off = range.start;
                        for raw_line in raw.split_inclusive('\n') {
                            let line = raw_line.trim_end_matches(['\r', '\n']);
                            let mut col = 0;
                            while let Some((gt, content)) = strip_quote_marker(&line[col..]) {
                                tokens.push(RawToken {
                                    range: off + col + gt..off + col + gt + 1,
                                    style: QUOTE_MARK,
                                });
                                col += content;
                            }
                            off += raw_line.len();
                        }
                    }
                }
                let mut pair = if count != 0 && range.len() >= 2 * count {
                    let open = range.start..range.start + count;
                    let close = range.end - count..range.end;
                    tokens.push(RawToken {
                        range: open.clone(),
                        style: PUNCTUATION,
                    });
                    Some((range.clone(), open, close))
                } else {
                    None
                };
                if let Tag::CodeBlock(CodeBlockKind::Fenced(ref info)) = tag
                    && let Some(&ch) = raw.as_bytes().first()
                {
                    let ticks = raw.bytes().take_while(|b| *b == ch).count();
                    if ticks >= 3 {
                        let first_line_end = raw.find('\n').unwrap_or(raw.len());
                        let header_end = raw[..first_line_end].trim_end_matches('\r').len();
                        tokens.push(RawToken {
                            range: range.start..range.start + header_end,
                            style: PUNCTUATION | CODE_BLOCK,
                        });
                        let tail = raw.trim_end_matches(['\n', '\r', ' ', '\t']);
                        let body_start = (range.start + first_line_end + usize::from(first_line_end < raw.len())).min(range.end);
                        let mut body_end = range.start + tail.len();
                        let mut close_hide = None;
                        if let Some((_, final_line)) = tail.rsplit_once('\n') {
                            let last = final_line.trim_start();
                            if last.len() >= ticks && last.bytes().all(|b| b == ch) {
                                let close =
                                    range.start + tail.len() - last.len()..range.start + tail.len();
                                let close_line_start = range.start + tail.len() - final_line.len();
                                body_end = close_line_start.saturating_sub(1).max(body_start);
                                if body_end > body_start && md.as_bytes().get(body_end - 1) == Some(&b'\r') {
                                    body_end -= 1;
                                }
                                close_hide = Some(if body_start < body_end {
                                    body_end..close.end
                                } else {
                                    close_line_start..close.end
                                });
                                pair = Some((
                                    range.clone(),
                                    range.start..range.start + header_end,
                                    close,
                                ));
                            }
                        }
                        if first_line_end < raw.len() {
                            groups.push(CollapseGroup {
                                outer: range.start..range.start + tail.len(),
                                open: range.start..body_start,
                                close: close_hide,
                                is_heading_prefix: false,
                                rich_only: true,
                            });
                        }
                        let lang = info.split_whitespace().next().unwrap_or("").to_owned();
                        let after_ticks = range.start + ticks;
                        let header_slice = &md[after_ticks..range.start + header_end];
                        let lead_ws = header_slice.len() - header_slice.trim_start().len();
                        let lang_start = after_ticks + lead_ws;
                        let lang_range = if lang.is_empty() {
                            after_ticks..after_ticks
                        } else {
                            lang_start..(lang_start + lang.len()).min(range.start + header_end)
                        };
                        code_blocks.push(ParsedCodeBlock {
                            range: range.start..range.start + tail.len(),
                            lang,
                            lang_range,
                            body_range: body_start..body_end.max(body_start),
                        });
                    }
                }
                if add == HEADING {
                    let raw_h = &md[range.clone()];
                    let lead = raw_h.len() - raw_h.trim_start_matches(' ').len();
                    let digits = raw_h[lead..].bytes().take_while(|b| *b == b'#').count();
                    if digits > 0 {
                        let hash_start = range.start + lead;
                        let hash_end = hash_start + digits;
                        let sep = usize::from(md[hash_end..range.end].starts_with([' ', '\t']));
                        let prefix_end = hash_end + sep;
                        groups.push(CollapseGroup {
                            outer: hash_start..range.end,
                            open: hash_start..prefix_end,
                            close: None,
                            is_heading_prefix: true,
                            rich_only: false,
                        });
                        tokens.push(RawToken {
                            range: hash_start..hash_end,
                            style: PUNCTUATION,
                        });
                    }
                }
                stack.push((inherited | add, pair));
                continue;
            }
            Event::End(tag) => {
                if matches!(tag, TagEnd::Link | TagEnd::Image)
                    && links.last().is_some_and(|a| a.outer == range)
                    && let Some(active) = links.pop()
                {
                    let label_start = (active.outer.start + active.open_len).min(active.outer.end);
                    let label_end = active.last_child.clamp(label_start, active.outer.end);
                    if active.open_len > 0 {
                        let open = active.outer.start..label_start;
                        let close = (label_end < active.outer.end)
                            .then_some(label_end..active.outer.end);
                        if let Some(c) = &close {
                            tokens.push(RawToken {
                                range: c.clone(),
                                style: PUNCTUATION,
                            });
                        }
                        groups.push(CollapseGroup::inline(active.outer.clone(), open, close));
                    }
                    if let Some((url, title, is_autolink)) = active.meta {
                        parsed_links.push(ParsedLink {
                            outer: active.outer,
                            label_range: label_start..label_end,
                            url,
                            title,
                            is_autolink,
                        });
                    }
                }
                if let Some((_, Some((outer, open, close)))) = stack.pop() {
                    let is_fence = md[open.clone()].starts_with("```")
                        || md[open.clone()].starts_with("~~~");
                    tokens.push(RawToken {
                        range: close.clone(),
                        style: if is_fence {
                            PUNCTUATION | CODE_BLOCK
                        } else {
                            PUNCTUATION
                        },
                    });
                    if !is_fence {
                        groups.push(CollapseGroup::inline(outer, open, Some(close)));
                    }
                }
                continue;
            }
            Event::Code(_) => {
                let ticks = md[range.clone()].bytes().take_while(|b| *b == b'`').count();
                if ticks > 0 && range.len() > 2 * ticks {
                    let open = range.start..range.start + ticks;
                    let close = range.end - ticks..range.end;
                    groups.push(CollapseGroup::inline(
                        range.clone(),
                        open.clone(),
                        Some(close.clone()),
                    ));
                    tokens.push(RawToken {
                        range: open,
                        style: PUNCTUATION,
                    });
                    tokens.push(RawToken {
                        range: range.start + ticks..range.end - ticks,
                        style: inherited | CODE,
                    });
                    range.start = range.end - ticks;
                    PUNCTUATION
                } else {
                    inherited | CODE
                }
            }
            Event::Rule => RULE,
            Event::Text(_) => {
                if inherited & (CODE | CODE_BLOCK | LINK) == 0 {
                    let slice = &md[range.clone()];
                    for url_range in find_bare_urls(slice, range.start) {
                        tokens.push(RawToken {
                            range: url_range.clone(),
                            style: inherited | LINK,
                        });
                        parsed_links.push(ParsedLink {
                            outer: url_range.clone(),
                            label_range: url_range.clone(),
                            url: md[url_range].to_owned(),
                            title: String::new(),
                            is_autolink: true,
                        });
                    }
                    for (outer, open, close) in find_highlights(slice, range.start) {
                        let inner = open.end..close.start;
                        groups.push(CollapseGroup::inline(
                            outer,
                            open.clone(),
                            Some(close.clone()),
                        ));
                        tokens.push(RawToken {
                            range: open,
                            style: PUNCTUATION | (inherited & CALLOUT_ANY),
                        });
                        tokens.push(RawToken {
                            range: inner,
                            style: inherited | HIGHLIGHT,
                        });
                        tokens.push(RawToken {
                            range: close,
                            style: PUNCTUATION | (inherited & CALLOUT_ANY),
                        });
                    }
                }
                // Plain body text inside a callout renders directly via native Parley
                // over the `.callout-bg` backdrop; only inline-styled runs need overlays.
                if inherited & CALLOUT_ANY != 0 && (inherited & !CALLOUT_ANY) == 0 {
                    0
                } else {
                    inherited
                }
            }
            _ => continue,
        };
        if style != 0 && !range.is_empty() {
            let in_callout_header = callouts
                .iter()
                .any(|c| range.start < c.header_line.end && range.end > c.tag_range.start);
            if !in_callout_header {
                let effective = if style == PUNCTUATION || style == BULLET {
                    style | (inherited & CALLOUT_ANY)
                } else {
                    style
                };
                tokens.push(RawToken {
                    range,
                    style: effective,
                });
            }
        }
    }
    for c in &callouts {
        let cbit = callout_style(c.kind);
        if let Some(tr) = &c.title_range {
            tokens.push(RawToken {
                range: tr.clone(),
                style: CALLOUT_TITLE | cbit,
            });
        }
    }
    apply_code_highlights(md, &code_blocks, &mut tokens);
    // Tag punctuation/bullet tokens that sit inside a callout so their overlay
    // background blends seamlessly into the callout wash.
    for t in &mut tokens {
        if t.style & CALLOUT_ANY == 0
            && let Some(c) = callouts
                .iter()
                .find(|c| t.range.start >= c.range.start && t.range.end <= c.range.end)
        {
            t.style |= callout_style(c.kind);
        }
    }
    groups.sort_by_key(|g| (g.outer.start, g.outer.end));
    tokens.sort_by_key(|t| (t.range.start, t.range.end));
    parsed_links.sort_by_key(|l| (l.outer.start, l.outer.end));
    ParsedMarkdown {
        groups,
        tokens,
        links: parsed_links,
        callouts,
        code_blocks,
    }
}

fn apply_code_highlights(md: &str, code_blocks: &[ParsedCodeBlock], tokens: &mut Vec<RawToken>) {
    let mut highlighted = Vec::new();
    for cb in code_blocks {
        if cb.lang.is_empty() || cb.body_range.is_empty() {
            continue;
        }
        let spans = highlight_code(&cb.lang, &md[cb.body_range.clone()]);
        if !spans.is_empty() {
            highlighted.push((cb.body_range.clone(), spans));
        }
    }
    if highlighted.is_empty() {
        return;
    }
    let old = std::mem::take(tokens);
    tokens.reserve(old.len() + highlighted.iter().map(|(_, s)| s.len() * 2).sum::<usize>());
    for tok in old {
        if tok.style & CODE_BLOCK == 0 || tok.style & PUNCTUATION != 0 {
            tokens.push(tok);
            continue;
        }
        let Some((body_range, spans)) = highlighted
            .iter()
            .find(|(br, _)| tok.range.start < br.end && tok.range.end > br.start)
        else {
            tokens.push(tok);
            continue;
        };
        let base = body_range.start;
        let first = spans.partition_point(|s| base + s.range.end <= tok.range.start);
        let mut at = tok.range.start;
        for span in &spans[first..] {
            let s_start = (base + span.range.start).max(tok.range.start);
            let s_end = (base + span.range.end).min(tok.range.end);
            if s_start >= tok.range.end {
                break;
            }
            if s_start >= s_end {
                continue;
            }
            if s_start > at {
                tokens.push(RawToken {
                    range: at..s_start,
                    style: tok.style,
                });
            }
            tokens.push(RawToken {
                range: s_start..s_end,
                style: tok.style | syntax_style(span.kind),
            });
            at = s_end;
        }
        if at < tok.range.end {
            tokens.push(RawToken {
                range: at..tok.range.end,
                style: tok.style,
            });
        }
    }
}

impl Projection {
    pub fn build(
        md: &str,
        parsed: &ParsedMarkdown,
        field_range: Range<usize>,
        markers: &[Marker],
        mode: EditMode,
        active_sel: Option<[usize; 2]>,
        mouse_box: Option<usize>,
        literal: Option<(usize, usize)>,
        is_body: bool,
        is_cell: bool,
    ) -> Self {
        let groups = &parsed.groups;
        let raw_tokens = &parsed.tokens;
        let sel_range = active_sel.map(|[a, b]| a.min(b)..a.max(b));
        let near_marker = if mode == EditMode::Markdown && is_body {
            sel_range
                .clone()
                .and_then(|r| marker_beside(markers, r))
                .filter(|m| mouse_box != Some(m.open))
        } else {
            None
        };

        // Determine which byte ranges inside `field_range` are collapsed/replaced.
        #[derive(Clone, Debug)]
        enum Override {
            Hidden(Range<usize>),
            HiddenPrefix(Range<usize>),
            Checkbox(Range<usize>),
            EscapedPipe(Range<usize>),
        }
        impl Override {
            fn range(&self) -> Range<usize> {
                match self {
                    Self::Hidden(r)
                    | Self::HiddenPrefix(r)
                    | Self::Checkbox(r)
                    | Self::EscapedPipe(r) => r.clone(),
                }
            }
        }

        let mut overrides: Vec<Override> = Vec::new();
        let mut hidden_pairs: Vec<DelimPair> = Vec::new();
        let mut has_hidden_heading_prefix = false;

        for g in groups {
            if g.outer.start >= field_range.end {
                break;
            }
            if g.outer.end <= field_range.start {
                continue;
            }
            let expanded = match mode {
                EditMode::Rich => false,
                EditMode::Markdown => {
                    if g.rich_only {
                        true
                    } else if g.is_heading_prefix {
                        active_sel.is_some()
                    } else {
                        sel_range
                            .as_ref()
                            .is_some_and(|s| s.start <= g.outer.end && s.end >= g.outer.start)
                    }
                }
            };
            if !expanded {
                let open = g.open.start.max(field_range.start)..g.open.end.min(field_range.end);
                if !open.is_empty() {
                    if g.is_heading_prefix && open.start == field_range.start {
                        has_hidden_heading_prefix = true;
                    }
                    if g.rich_only {
                        overrides.push(Override::HiddenPrefix(open.clone()));
                    } else {
                        overrides.push(Override::Hidden(open.clone()));
                    }
                }
                if let Some(close) = &g.close {
                    let c = close.start.max(field_range.start)..close.end.min(field_range.end);
                    if !c.is_empty() {
                        overrides.push(Override::Hidden(c.clone()));
                        if !open.is_empty() && !g.rich_only {
                            hidden_pairs.push(DelimPair {
                                outer: g.outer.clone(),
                                open,
                                close: c,
                            });
                        }
                    }
                }
            }
        }

        let mut checkboxes_meta: Vec<(usize, bool, bool)> = Vec::new();
        if is_body {
            let from = markers.partition_point(|m| m.open + 3 <= field_range.start);
            for m in &markers[from..] {
                if m.open >= field_range.end {
                    break;
                }
                if m.open < field_range.start || m.open + 3 > field_range.end {
                    continue;
                }
                let revealed = mode == EditMode::Markdown
                    && (near_marker.is_some_and(|n| n.open == m.open)
                        || literal.is_some_and(|(p, _)| p == m.open));
                checkboxes_meta.push((m.open, m.checked, revealed));
                if !revealed {
                    overrides.push(Override::Checkbox(m.open..m.open + 3));
                }
            }
        }

        if is_cell {
            let raw = &md[field_range.clone()];
            let mut at = 0;
            while let Some(pos) = raw[at..].find("\\|") {
                let abs = field_range.start + at + pos;
                overrides.push(Override::EscapedPipe(abs..abs + 2));
                at += pos + 2;
            }
        }

        overrides.sort_by_key(|o| (o.range().start, o.range().end));

        let mut display = String::with_capacity(field_range.len());
        let mut pieces = Vec::new();
        let mut cursor = field_range.start;

        for ov in overrides {
            let r = ov.range();
            if r.start < cursor || r.end > field_range.end {
                continue;
            }
            if r.start > cursor {
                let d_start = display.len();
                display.push_str(&md[cursor..r.start]);
                pieces.push(Piece {
                    source: cursor..r.start,
                    display: d_start..display.len(),
                    kind: PieceKind::Text,
                });
            }
            let d_start = display.len();
            match ov {
                Override::Hidden(_) => {
                    pieces.push(Piece {
                        source: r.clone(),
                        display: d_start..d_start,
                        kind: PieceKind::Hidden,
                    });
                }
                Override::HiddenPrefix(_) => {
                    pieces.push(Piece {
                        source: r.clone(),
                        display: d_start..d_start,
                        kind: PieceKind::HiddenPrefix,
                    });
                }
                Override::Checkbox(_) => {
                    display.push_str(CHECKBOX_SLOT_MONO);
                    pieces.push(Piece {
                        source: r.clone(),
                        display: d_start..display.len(),
                        kind: PieceKind::Checkbox(r.start),
                    });
                }
                Override::EscapedPipe(_) => {
                    display.push('|');
                    pieces.push(Piece {
                        source: r.clone(),
                        display: d_start..display.len(),
                        kind: PieceKind::EscapedPipe,
                    });
                }
            }
            cursor = r.end;
        }
        if cursor < field_range.end {
            let d_start = display.len();
            display.push_str(&md[cursor..field_range.end]);
            pieces.push(Piece {
                source: cursor..field_range.end,
                display: d_start..display.len(),
                kind: PieceKind::Text,
            });
        }

        let proj = Self {
            field_start: field_range.start,
            display,
            pieces,
            tokens: Vec::new(),
            checkboxes: Vec::new(),
            hidden_pairs,
            has_hidden_heading_prefix,
            is_cell,
        };

        // Map styled tokens into display byte ranges.
        let mut disp_tokens = Vec::new();
        let start_idx = raw_tokens.partition_point(|t| t.range.end <= field_range.start);
        for t in raw_tokens[start_idx..]
            .iter()
            .take_while(|t| t.range.start < field_range.end)
        {
            let style = if is_body { t.style } else { t.style & INLINE };
            if style == 0 {
                continue;
            }
            let s_start = t.range.start.max(field_range.start);
            let s_end = t.range.end.min(field_range.end);
            if s_start >= s_end {
                continue;
            }
            let d_start = proj.source_to_display(s_start);
            let d_end = proj.source_to_display(s_end);
            if d_start < d_end {
                disp_tokens.push(DisplayToken {
                    display: d_start..d_end,
                    source_start: s_start,
                    style,
                });
            }
        }
        disp_tokens.sort_by_key(|t| (t.display.start, t.display.end));

        let checkboxes = checkboxes_meta
            .into_iter()
            .map(|(open, checked, revealed)| {
                let d_start = proj.source_to_display(open);
                let d_end = proj.source_to_display(open + 3);
                DisplayCheckbox {
                    open,
                    checked,
                    display: d_start..d_end,
                    revealed,
                }
            })
            .collect();

        Self {
            tokens: disp_tokens,
            checkboxes,
            ..proj
        }
    }

    /// Map an absolute Markdown byte offset to a display byte offset inside this field.
    pub fn source_to_display(&self, src: usize) -> usize {
        if src <= self.field_start {
            return 0;
        }
        for p in &self.pieces {
            if src < p.source.start {
                return p.display.start;
            }
            if src <= p.source.end {
                return match p.kind {
                    PieceKind::Text => p.display.start + (src - p.source.start),
                    PieceKind::Hidden | PieceKind::HiddenPrefix => p.display.start,
                    PieceKind::Checkbox(_) => {
                        if src <= p.source.start {
                            p.display.start
                        } else {
                            p.display.end
                        }
                    }
                    PieceKind::EscapedPipe => {
                        if src <= p.source.start {
                            p.display.start
                        } else {
                            p.display.end
                        }
                    }
                };
            }
        }
        self.display.len()
    }

    /// Map a display byte offset inside this field to an absolute Markdown byte offset.
    pub fn display_to_source(&self, disp: usize) -> usize {
        let disp = self.display.floor_char_boundary(disp.min(self.display.len()));
        if self.pieces.is_empty() {
            return self.field_start;
        }
        for (i, p) in self.pieces.iter().enumerate() {
            if disp < p.display.start {
                return p.source.start;
            }
            if disp < p.display.end {
                return match p.kind {
                    PieceKind::Text => p.source.start + (disp - p.display.start),
                    PieceKind::Checkbox(_) | PieceKind::EscapedPipe => p.source.start,
                    PieceKind::Hidden | PieceKind::HiddenPrefix => p.source.start,
                };
            }
            if disp == p.display.end
                && !matches!(p.kind, PieceKind::Hidden | PieceKind::HiddenPrefix)
            {
                // When followed by a hidden block-opening prefix (e.g. a code fence header
                // in Rich mode), a caret at the seam belongs inside the content after it.
                if let Some(next) = self.pieces.get(i + 1)
                    && next.kind == PieceKind::HiddenPrefix
                {
                    return next.source.end;
                }
                return p.source.end;
            }
        }
        self.pieces
            .last()
            .map_or(self.field_start, |p| p.source.end)
    }

    /// Convert a display-buffer text change into source splices and the resulting source caret.
    pub fn map_input(&self, new_display: &str) -> Option<(Vec<Splice>, usize)> {
        let (removed, inserted) = text_change(&self.display, new_display);
        if removed.is_empty() && inserted.is_empty() {
            return None;
        }
        let encoded_insert = if self.is_cell {
            table::encoded(inserted)
        } else {
            inserted.to_owned()
        };

        if removed.is_empty() {
            let at = self.display_to_source(removed.start);
            let next_caret = at + encoded_insert.len();
            return Some((vec![Splice::new(at..at, encoded_insert)], next_caret));
        }

        // Deletion / replacement across pieces.
        let mut src_start = self.display_to_source_edge(removed.start, true);
        let mut src_end = self.display_to_source_edge(removed.end, false);
        if src_end < src_start {
            std::mem::swap(&mut src_start, &mut src_end);
        }

            // Whole-field replacement (e.g. Select All + type) replaces leading/trailing hidden syntax too,
            // except a hidden heading prefix in Rich mode so replacing a heading's text keeps its level.
            if removed.start == 0 && removed.end == self.display.len() {
                if !self.has_hidden_heading_prefix {
                    src_start = self.field_start;
                }
                if let Some(last) = self.pieces.last() {
                    src_end = src_end.max(last.source.end);
                }
            }

            // If removing or replacing a collapsed checkbox slot, also strip a preceding hidden task dash (`- `).
            for (i, p) in self.pieces.iter().enumerate() {
                if let PieceKind::Checkbox(_) = p.kind
                    && removed.start <= p.display.start
                    && removed.end >= p.display.end
                    && i > 0
                    && self.pieces[i - 1].kind == PieceKind::Hidden
                    && self.pieces[i - 1].source.end == p.source.start
                {
                    src_start = src_start.min(self.pieces[i - 1].source.start);
                }
            }

            // If deleting all visible text inside a hidden delimiter pair, remove the pair too.
            if encoded_insert.is_empty() {
                for pair in &self.hidden_pairs {
                    if src_start <= pair.open.end && src_end >= pair.close.start {
                        src_start = src_start.min(pair.open.start);
                        src_end = src_end.max(pair.close.end);
                    }
                }
            }

        let next_caret = src_start + encoded_insert.len();
        Some((
            vec![Splice::new(src_start..src_end, encoded_insert)],
            next_caret,
        ))
    }

    fn display_to_source_edge(&self, disp: usize, is_start: bool) -> usize {
        let disp = self.display.floor_char_boundary(disp.min(self.display.len()));
        for p in &self.pieces {
            if matches!(p.kind, PieceKind::Hidden | PieceKind::HiddenPrefix) {
                continue;
            }
            if is_start && disp >= p.display.start && disp < p.display.end {
                return match p.kind {
                    PieceKind::Text => p.source.start + (disp - p.display.start),
                    PieceKind::Checkbox(_) | PieceKind::EscapedPipe => p.source.start,
                    PieceKind::Hidden | PieceKind::HiddenPrefix => unreachable!(),
                };
            }
            if !is_start && disp > p.display.start && disp <= p.display.end {
                return match p.kind {
                    PieceKind::Text => p.source.start + (disp - p.display.start),
                    PieceKind::Checkbox(_) | PieceKind::EscapedPipe => p.source.end,
                    PieceKind::Hidden | PieceKind::HiddenPrefix => unreachable!(),
                };
            }
        }
        self.display_to_source(disp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use margin_editor_core::checkbox::markdown_markers;

    #[test]
    fn markdown_mode_is_tight_at_rest_and_expands_only_the_active_span() {
        let md = "Leave **ten seconds** and `48 kHz` here.";
        let parsed = parse_markdown(md);
        let markers = markdown_markers(md);
        let resting = Projection::build(
            md,
            &parsed,
            0..md.len(),
            &markers,
            EditMode::Markdown,
            Some([0, 0]),
            None,
            None,
            true,
            false,
        );
        assert_eq!(resting.display, "Leave ten seconds and 48 kHz here.");

        // Cursor inside **ten seconds** expands ONLY bold, keeping `48 kHz` tight.
        let inside_bold = md.find("seconds").unwrap();
        let active = Projection::build(
            md,
            &parsed,
            0..md.len(),
            &markers,
            EditMode::Markdown,
            Some([inside_bold, inside_bold]),
            None,
            None,
            true,
            false,
        );
        assert_eq!(active.display, "Leave **ten seconds** and 48 kHz here.");

        // Rich mode stays tight even when cursor is inside **ten seconds**.
        let rich = Projection::build(
            md,
            &parsed,
            0..md.len(),
            &markers,
            EditMode::Rich,
            Some([inside_bold, inside_bold]),
            None,
            None,
            true,
            false,
        );
        assert_eq!(rich.display, "Leave ten seconds and 48 kHz here.");
    }

    #[test]
    fn mouse_clicked_checkbox_stays_togglebox_while_keyboard_cursor_reveals_it() {
        let md = "- [ ] Pack\n- [x] Done";
        let parsed = parse_markdown(md);
        let markers = markdown_markers(md);
        let open = md.find("[ ]").unwrap();

        let mouse = Projection::build(
            md,
            &parsed,
            0..md.len(),
            &markers,
            EditMode::Markdown,
            Some([open + 1, open + 1]),
            Some(open),
            None,
            true,
            false,
        );
        assert!(!mouse.checkboxes[0].revealed);
        assert_eq!(
            mouse.display,
            format!("- {CHECKBOX_SLOT_MONO} Pack\n- {CHECKBOX_SLOT_MONO} Done")
        );

        let keyboard = Projection::build(
            md,
            &parsed,
            0..md.len(),
            &markers,
            EditMode::Markdown,
            Some([open + 1, open + 1]),
            None,
            None,
            true,
            false,
        );
        assert!(keyboard.checkboxes[0].revealed);
        assert!(!keyboard.checkboxes[1].revealed);
        assert_eq!(
            keyboard.display,
            format!("- [ ] Pack\n- {CHECKBOX_SLOT_MONO} Done")
        );
    }

    #[test]
    fn rich_editing_preserves_and_cleans_up_hidden_markdown_delimiters() {
        let md = "Leave **ten** here.";
        let parsed = parse_markdown(md);
        let markers = markdown_markers(md);
        let proj = Projection::build(
            md,
            &parsed,
            0..md.len(),
            &markers,
            EditMode::Rich,
            Some([9, 9]),
            None,
            None,
            true,
            false,
        );
        assert_eq!(proj.display, "Leave ten here.");
        // Typing inside "ten" -> "tens"
        let (splices, _) = proj.map_input("Leave tens here.").unwrap();
        let mut out = md.to_owned();
        for s in splices.into_iter().rev() {
            out.replace_range(s.range, &s.text);
        }
        assert_eq!(out, "Leave **tens** here.");

        // Deleting all of "ten" cleans up the empty ** **
        let (splices, _) = proj.map_input("Leave  here.").unwrap();
        let mut out = md.to_owned();
        for s in splices.into_iter().rev() {
            out.replace_range(s.range, &s.text);
        }
        assert_eq!(out, "Leave  here.");
    }

    #[test]
    fn rich_mode_hides_code_fences_and_edits_inside_body_cleanly() {
        let md = "Before\n\n```rust\nfn main() {}\n```\n\nAfter";
        let parsed = parse_markdown(md);
        let markers = markdown_markers(md);
        let live = Projection::build(
            md,
            &parsed,
            0..md.len(),
            &markers,
            EditMode::Markdown,
            None,
            None,
            None,
            true,
            false,
        );
        assert!(live.display.contains("```rust"));
        let rich = Projection::build(
            md,
            &parsed,
            0..md.len(),
            &markers,
            EditMode::Rich,
            None,
            None,
            None,
            true,
            false,
        );
        assert_eq!(rich.display, "Before\n\nfn main() {}\n\nAfter");

        // Typing at the start of line 0 of the code block inserts inside the code block.
        let (splices, _) = rich.map_input("Before\n\npub fn main() {}\n\nAfter").unwrap();
        let mut out = md.to_owned();
        for s in splices.into_iter().rev() {
            out.replace_range(s.range, &s.text);
        }
        assert_eq!(out, "Before\n\n```rust\npub fn main() {}\n```\n\nAfter");

        // Empty code block in Rich mode retains 1 editable empty line and accepts typing inside.
        let empty = "```\n\n```";
        let ep = parse_markdown(empty);
        let erich = Projection::build(
            empty,
            &ep,
            0..empty.len(),
            &[],
            EditMode::Rich,
            None,
            None,
            None,
            true,
            false,
        );
        assert_eq!(erich.display, "\n");
        let (splices, _) = erich.map_input("let x = 1;\n").unwrap();
        let mut eout = empty.to_owned();
        for s in splices.into_iter().rev() {
            eout.replace_range(s.range, &s.text);
        }
        assert_eq!(eout, "```\nlet x = 1;\n```");
    }
}
