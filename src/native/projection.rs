//! Projection between raw Markdown UTF-8 bytes and the tight display text
//! rendered in native inputs.
//!
//! - In `EditMode::Rich` (Traditional), inline delimiters stay collapsed even
//!   when the caret is inside a formatted span; checkboxes always stay visual.
//! - In `EditMode::Markdown` (Keyboard-first Live), unfocused spans render
//!   tightly with zero ghost gaps, while the span containing the cursor expands
//!   inline so its Markdown delimiters can be edited directly.
use super::markdown::{
    BULLET, CODE, CODE_BLOCK, DONE, HEADING, ITALIC, LINK, PUNCTUATION, QUOTE, QUOTE_MARK, RULE,
    STRONG, STRIKE,
};
use margin_editor_core::{
    checkbox::{Marker, marker_beside},
    edit::{Splice, text_change},
    table,
};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

/// 3-byte UTF-8 EM SPACE used in proportional Rich mode (1em square checkbox slot).
pub const CHECKBOX_SLOT_RICH: &str = "\u{2003}";
/// 2 ASCII spaces used in monospace Markdown mode (~18px slot for a 15×15 square checkbox).
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
    pub style: u16,
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
    pub is_cell: bool,
}

#[derive(Clone, Debug)]
struct RawToken {
    range: Range<usize>,
    style: u16,
}

#[derive(Clone, Debug)]
struct CollapseGroup {
    outer: Range<usize>,
    open: Range<usize>,
    close: Option<Range<usize>>,
    is_task_dash: bool,
}

fn parse_groups_and_tokens(md: &str) -> (Vec<CollapseGroup>, Vec<RawToken>) {
    let mut groups = Vec::new();
    let mut tokens = Vec::new();
    let mut links: Vec<(Range<usize>, usize, usize)> = Vec::new(); // (outer, open_len, last_child_end)
    let mut stack: Vec<(u16, Option<(Range<usize>, Range<usize>, Range<usize>)>)> = vec![(0, None)];

    for (event, mut range) in Parser::new_ext(
        md,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    )
    .into_offset_iter()
    {
        let inherited = stack.last().unwrap().0;
        for (outer, _, last_child) in &mut links {
            if range != *outer && range.start >= outer.start && range.end <= outer.end {
                *last_child = (*last_child).max(range.end);
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
                    Tag::BlockQuote(_) => QUOTE,
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
                if matches!(tag, Tag::Link { .. } | Tag::Image { .. }) {
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
                        links.push((range.clone(), opening, range.start + opening));
                    }
                }
                if matches!(tag, Tag::Item) && raw.starts_with(['-', '*', '+']) {
                    let after_dash = &raw[1..];
                    let trimmed = after_dash.trim_start_matches([' ', '\t']);
                    let ws = after_dash.len() - trimmed.len();
                    let is_task = ["[ ]", "[x]", "[X]"]
                        .iter()
                        .any(|p| trimmed.starts_with(p));
                    if trimmed.starts_with("[x]") || trimmed.starts_with("[X]") {
                        add |= DONE;
                    }
                    if is_task {
                        let box_start = range.start + 1 + ws;
                        groups.push(CollapseGroup {
                            outer: range.start..box_start + 3,
                            open: range.start..box_start,
                            close: None,
                            is_task_dash: true,
                        });
                        tokens.push(RawToken {
                            range: range.start..range.start + 1,
                            style: PUNCTUATION,
                        });
                    } else {
                        tokens.push(RawToken {
                            range: range.start..range.start + 1,
                            style: BULLET,
                        });
                    }
                }
                if matches!(tag, Tag::BlockQuote(_)) && raw.starts_with('>') {
                    tokens.push(RawToken {
                        range: range.start..range.start + 1,
                        style: QUOTE_MARK,
                    });
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
                if matches!(tag, Tag::CodeBlock(CodeBlockKind::Fenced(_)))
                    && let Some(&ch) = raw.as_bytes().first()
                {
                    let ticks = raw.bytes().take_while(|b| *b == ch).count();
                    if ticks >= 3 {
                        let first_line_end = raw.find('\n').unwrap_or(raw.len());
                        let header_end = raw[..first_line_end].trim_end_matches('\r').len();
                        tokens.push(RawToken {
                            range: range.start..range.start + header_end,
                            style: PUNCTUATION,
                        });
                        let tail = raw.trim_end_matches(['\n', '\r', ' ', '\t']);
                        if let Some((_, final_line)) = tail.rsplit_once('\n') {
                            let last = final_line.trim_start();
                            if last.len() >= ticks && last.bytes().all(|b| b == ch) {
                                let close =
                                    range.start + tail.len() - last.len()..range.start + tail.len();
                                pair = Some((
                                    range.clone(),
                                    range.start..range.start + header_end,
                                    close,
                                ));
                            }
                        }
                    }
                }
                if add == HEADING {
                    let digits = md[range.clone()].bytes().take_while(|b| *b == b'#').count();
                    if digits > 0 {
                        tokens.push(RawToken {
                            range: range.start..range.start + digits,
                            style: PUNCTUATION,
                        });
                    }
                }
                stack.push((inherited | add, pair));
                continue;
            }
            Event::End(tag) => {
                if matches!(tag, TagEnd::Link | TagEnd::Image)
                    && links.last().is_some_and(|(outer, _, _)| *outer == range)
                    && let Some((outer, open_len, last)) = links.pop()
                {
                    let open = outer.start..outer.start + open_len;
                    let close = (last < outer.end).then_some(last..outer.end);
                    if let Some(c) = &close {
                        tokens.push(RawToken {
                            range: c.clone(),
                            style: PUNCTUATION,
                        });
                    }
                    groups.push(CollapseGroup {
                        outer,
                        open,
                        close,
                        is_task_dash: false,
                    });
                }
                if let Some((_, Some((outer, open, close)))) = stack.pop() {
                    let is_fence = md[open.clone()].starts_with("```")
                        || md[open.clone()].starts_with("~~~");
                    tokens.push(RawToken {
                        range: close.clone(),
                        style: PUNCTUATION,
                    });
                    if !is_fence {
                        groups.push(CollapseGroup {
                            outer,
                            open,
                            close: Some(close),
                            is_task_dash: false,
                        });
                    }
                }
                continue;
            }
            Event::Code(_) => {
                let ticks = md[range.clone()].bytes().take_while(|b| *b == b'`').count();
                if ticks > 0 && range.len() > 2 * ticks {
                    let open = range.start..range.start + ticks;
                    let close = range.end - ticks..range.end;
                    groups.push(CollapseGroup {
                        outer: range.clone(),
                        open: open.clone(),
                        close: Some(close.clone()),
                        is_task_dash: false,
                    });
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
            Event::Text(_) => inherited,
            _ => continue,
        };
        if style != 0 && !range.is_empty() {
            tokens.push(RawToken { range, style });
        }
    }
    tokens.sort_by_key(|t| (t.range.start, t.range.end));
    (groups, tokens)
}

impl Projection {
    pub fn build(
        md: &str,
        field_range: Range<usize>,
        markers: &[Marker],
        mode: EditMode,
        active_sel: Option<[usize; 2]>,
        mouse_box: Option<usize>,
        literal: Option<(usize, usize)>,
        is_body: bool,
        is_cell: bool,
    ) -> Self {
        let (groups, raw_tokens) = parse_groups_and_tokens(md);
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
            Checkbox(Range<usize>),
            EscapedPipe(Range<usize>),
        }
        impl Override {
            fn range(&self) -> Range<usize> {
                match self {
                    Self::Hidden(r) | Self::Checkbox(r) | Self::EscapedPipe(r) => r.clone(),
                }
            }
        }

        let mut overrides: Vec<Override> = Vec::new();
        let mut hidden_pairs: Vec<DelimPair> = Vec::new();

        for g in &groups {
            if g.outer.end <= field_range.start || g.outer.start >= field_range.end {
                continue;
            }
            let expanded = match mode {
                EditMode::Rich => false,
                EditMode::Markdown => {
                    if g.is_task_dash {
                        let box_open = g.open.end;
                        near_marker.is_some_and(|m| m.open == box_open)
                            || literal.is_some_and(|(p, _)| p == box_open)
                            || sel_range.as_ref().is_some_and(|s| {
                                s.start < g.open.end && s.end >= g.open.start
                            })
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
                    overrides.push(Override::Hidden(open.clone()));
                }
                if let Some(close) = &g.close {
                    let c = close.start.max(field_range.start)..close.end.min(field_range.end);
                    if !c.is_empty() {
                        overrides.push(Override::Hidden(c.clone()));
                        if !open.is_empty() {
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
                Override::Checkbox(_) => {
                    display.push_str(if mode == EditMode::Rich {
                        CHECKBOX_SLOT_RICH
                    } else {
                        CHECKBOX_SLOT_MONO
                    });
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
            is_cell,
        };

        // Map styled tokens into display byte ranges.
        const INLINE: u16 = STRONG | ITALIC | CODE | LINK | STRIKE | PUNCTUATION;
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
                    PieceKind::Hidden => p.display.start,
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
                    PieceKind::Hidden => p.source.start,
                };
            }
            if disp == p.display.end && p.kind != PieceKind::Hidden {
                // If followed by a hidden closing delimiter, prefer staying at the end of the text
                // piece so typing continues cleanly inside the active word; if followed by a hidden
                // opening delimiter, stay before the opening delimiter.
                let next_is_hidden = self
                    .pieces
                    .get(i + 1)
                    .is_some_and(|n| n.kind == PieceKind::Hidden);
                if !next_is_hidden {
                    return p.source.end;
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

            // Whole-field replacement (e.g. Select All + type) replaces leading/trailing hidden syntax too.
            if removed.start == 0 && removed.end == self.display.len() {
                src_start = self.field_start;
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
            if p.kind == PieceKind::Hidden {
                continue;
            }
            if is_start && disp >= p.display.start && disp < p.display.end {
                return match p.kind {
                    PieceKind::Text => p.source.start + (disp - p.display.start),
                    PieceKind::Checkbox(_) | PieceKind::EscapedPipe => p.source.start,
                    PieceKind::Hidden => unreachable!(),
                };
            }
            if !is_start && disp > p.display.start && disp <= p.display.end {
                return match p.kind {
                    PieceKind::Text => p.source.start + (disp - p.display.start),
                    PieceKind::Checkbox(_) | PieceKind::EscapedPipe => p.source.end,
                    PieceKind::Hidden => unreachable!(),
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
        let markers = markdown_markers(md);
        let resting = Projection::build(
            md,
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
        let markers = markdown_markers(md);
        let open = md.find("[ ]").unwrap();

        let mouse = Projection::build(
            md,
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
            format!("{CHECKBOX_SLOT_MONO} Pack\n{CHECKBOX_SLOT_MONO} Done")
        );

        let keyboard = Projection::build(
            md,
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
            format!("- [ ] Pack\n{CHECKBOX_SLOT_MONO} Done")
        );
    }

    #[test]
    fn rich_editing_preserves_and_cleans_up_hidden_markdown_delimiters() {
        let md = "Leave **ten** here.";
        let markers = markdown_markers(md);
        let proj = Projection::build(
            md,
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
}
