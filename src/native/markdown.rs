//! Presentation ranges live in the same UTF-8 source coordinate space as Parley and Loro.
use super::{Field, Writing};
use dioxus_native::prelude::*;
use parley::Cluster;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

pub(super) const STRONG: u16 = 1;
pub(super) const ITALIC: u16 = 2;
pub(super) const CODE: u16 = 4;
pub(super) const LINK: u16 = 8;
pub(super) const STRIKE: u16 = 16;
pub(super) const HEADING: u16 = 32;
pub(super) const QUOTE: u16 = 64;
pub(super) const RULE: u16 = 128;
pub(super) const PUNCTUATION: u16 = 256;
pub(super) const BULLET: u16 = 512;
pub(super) const TASK_DASH: u16 = 1024;
pub(super) const QUOTE_MARK: u16 = 2048;
pub(super) const DONE: u16 = 4096;
pub(super) const CODE_BLOCK: u16 = 8192;

#[derive(Clone, PartialEq)]
pub struct Token {
    range: Range<usize>,
    style: u16,
}

pub fn syntax(md: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut links: Vec<(Range<usize>, usize)> = Vec::new();
    let mut stack = vec![(0u16, None)];
    for (event, mut range) in Parser::new_ext(
        md,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS,
    )
    .into_offset_iter()
    {
        let inherited = stack.last().unwrap().0;
        for (outer, last_child) in &mut links {
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
                        tokens.push(Token {
                            range: range.start..range.start + opening,
                            style: PUNCTUATION,
                        });
                        links.push((range.clone(), range.start + opening));
                    }
                }
                if matches!(tag, Tag::Item) && raw.starts_with(['-', '*', '+']) {
                    let rest = raw[1..].trim_start();
                    let is_task = ["[ ]", "[x]", "[X]"].iter().any(|p| rest.starts_with(p));
                    if rest.starts_with("[x]") || rest.starts_with("[X]") {
                        add |= DONE;
                    }
                    tokens.push(Token {
                        range: range.start..range.start + 1,
                        style: if is_task { TASK_DASH } else { BULLET },
                    });
                }
                if matches!(tag, Tag::BlockQuote(_)) && raw.starts_with('>') {
                    tokens.push(Token {
                        range: range.start..range.start + 1,
                        style: QUOTE_MARK,
                    });
                }
                let mut suffix = if count != 0 && range.len() >= 2 * count {
                    tokens.push(Token {
                        range: range.start..range.start + count,
                        style: PUNCTUATION,
                    });
                    Some(range.end - count..range.end)
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
                        tokens.push(Token {
                            range: range.start..range.start + header_end,
                            style: PUNCTUATION,
                        });
                        let tail = raw.trim_end_matches(['\n', '\r', ' ', '\t']);
                        if let Some((_, final_line)) = tail.rsplit_once('\n') {
                            let last = final_line.trim_start();
                            if last.len() >= ticks && last.bytes().all(|b| b == ch) {
                                suffix = Some(
                                    range.start + tail.len() - last.len()..range.start + tail.len(),
                                );
                            }
                        }
                    }
                }
                if add == HEADING {
                    let digits = md[range.clone()].bytes().take_while(|b| *b == b'#').count();
                    if digits > 0 {
                        tokens.push(Token {
                            range: range.start..range.start + digits,
                            style: PUNCTUATION,
                        });
                    }
                }
                stack.push((inherited | add, suffix));
                continue;
            }
            Event::End(tag) => {
                if matches!(tag, TagEnd::Link | TagEnd::Image)
                    && links.last().is_some_and(|(outer, _)| *outer == range)
                    && let Some((outer, last)) = links.pop()
                    && last < outer.end
                {
                    tokens.push(Token {
                        range: last..outer.end,
                        style: PUNCTUATION,
                    });
                }
                if let Some((_, Some(range))) = stack.pop() {
                    tokens.push(Token {
                        range,
                        style: PUNCTUATION,
                    });
                }
                continue;
            }
            Event::Code(_) => {
                let ticks = md[range.clone()].bytes().take_while(|b| *b == b'`').count();
                if ticks > 0 && range.len() > 2 * ticks {
                    tokens.push(Token {
                        range: range.start..range.start + ticks,
                        style: PUNCTUATION,
                    });
                    tokens.push(Token {
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
            tokens.push(Token { range, style });
        }
    }
    tokens.sort_by_key(|t| (t.range.start, t.range.end));
    tokens
}

#[derive(Clone, PartialEq)]
pub struct Ink {
    pub field: Field,
    at: usize,
    style: u16,
    text: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Ink {
    pub fn draw(self) -> Element {
        let Self {
            at,
            style,
            text,
            x,
            y,
            width,
            height,
            ..
        } = self;
        rsx! {
            span {
                key: "{at}", id: "ink-{at}", class: "md-ink", aria_hidden: "true",
                class: if style & (STRONG | HEADING) != 0 { "strong" },
                class: if style & ITALIC != 0 { "emphasis" },
                class: if style & CODE != 0 { "code" },
                class: if style & CODE_BLOCK != 0 { "code-block" },
                class: if style & LINK != 0 { "link" },
                class: if style & STRIKE != 0 { "strike" },
                class: if style & QUOTE != 0 { "quote" },
                class: if style & QUOTE_MARK != 0 { "quote-mark" },
                class: if style & RULE != 0 { "rule" },
                class: if style & PUNCTUATION != 0 { "punctuation" },
                class: if style & TASK_DASH != 0 { "task-dash" },
                class: if style & DONE != 0 { "done" },
                style: "left:{x}px; top:{y}px; width:{width}px; height:{height}px; line-height:{height}px;",
                if style == BULLET {
                    "•"
                } else if style == QUOTE_MARK {
                    span { class: "quote-bar" }
                } else if style == RULE {
                    span { class: "rule-line" }
                } else if style != TASK_DASH {
                    "{text}"
                }
            }
        }
    }
}

const INLINE_STYLES: u16 = STRONG | ITALIC | CODE | LINK | STRIKE | PUNCTUATION;

impl Writing {
    pub fn inline_fields(self) -> Vec<Field> {
        let mut fields = Vec::new();
        for token in self
            .syntax
            .read()
            .iter()
            .filter(|t| t.style & INLINE_STYLES != 0)
        {
            let field = self.field_at(token.range.start);
            if !matches!(field, Field::Body(_)) && !fields.contains(&field) {
                fields.push(field);
            }
        }
        fields
    }

    pub fn measured_ink(self, field: Field) -> Vec<Ink> {
        let handle = self.root.peek();
        let Some(handle) = handle.as_ref() else {
            return Vec::new();
        };
        let doc = handle.doc();
        let Some(node) = doc
            .get_element_by_id(&field.id())
            .and_then(|id| doc.get_node(id))
        else {
            return Vec::new();
        };
        let Some(input) = node.element_data().and_then(|e| e.text_input_data()) else {
            return Vec::new();
        };
        let Some(layout) = input.editor.try_layout() else {
            return Vec::new();
        };
        let text = input.editor.raw_text();
        let proj = self.projection(field);
        if input.editor.is_composing() || text != proj.display {
            return Vec::new();
        }
        let selection = input.editor.raw_selection().text_range();
        let whole_cell = matches!(field, Field::Cell(..))
            && self.cells.peek().is_some()
            && *self.active.peek() == Some(field)
            && !*self.cell_editing.peek();
        let scale = layout.scale();
        let top = if input.is_multiline {
            input.scroll_offset * scale
        } else {
            0.0
        };
        let l = node.final_layout();
        let left_inset = l.border.left + l.padding.left
            - if input.is_multiline || whole_cell {
                0.0
            } else {
                input.scroll_offset
            };
        let top_inset = l.border.top
            + l.padding.top
            + node.text_input_v_centering_offset(f64::from(scale)) as f32;
        let bottom = top + l.content_box_height() * scale;
        let tokens = &proj.tokens;
        let mut result = Vec::new();
        for line in layout.lines() {
            let m = line.metrics();
            if m.block_max_coord < top {
                continue;
            }
            if m.block_min_coord > bottom {
                break;
            }
            for run in line.runs() {
                // Preserve native bidi shaping, selected glyphs and tabs at their exact positions.
                if run.is_rtl() {
                    continue;
                }
                let run_range = run.text_range();
                let first = tokens.partition_point(|t| t.display.end <= run_range.start);
                for token in tokens[first..]
                    .iter()
                    .take_while(|t| t.display.start < run_range.end)
                {
                    let start = run_range.start.max(token.display.start);
                    let end = run_range.end.min(token.display.end);
                    let slice = text[start..end].trim_end_matches(['\n', '\r']);
                    if slice.is_empty()
                        || slice.contains('\t')
                        || (token.style == PUNCTUATION
                            && node.is_focussed()
                            && !whole_cell
                            && selection.start <= start + slice.len()
                            && selection.end >= start)
                        || (token.style != PUNCTUATION
                            && node.is_focussed()
                            && !whole_cell
                            && !selection.is_empty()
                            && selection.start < start + slice.len()
                            && selection.end > start)
                    {
                        continue;
                    }
                    let Some(left) = Cluster::from_byte_index(layout, start) else {
                        continue;
                    };
                    let Some(right) = Cluster::from_byte_index(layout, start + slice.len() - 1)
                    else {
                        continue;
                    };
                    let (Some(x), Some(last_x)) = (left.visual_offset(), right.visual_offset())
                    else {
                        continue;
                    };
                    let width = if token.style == RULE {
                        l.content_box_width() - x / scale
                    } else {
                        (last_x + right.advance() - x) / scale
                    };
                    result.push(Ink {
                        field,
                        at: token.source_start + (start - token.display.start),
                        style: token.style,
                        text: slice.to_owned(),
                        x: left_inset + x / scale,
                        y: top_inset + (m.block_min_coord - top) / scale,
                        width,
                        height: (m.block_max_coord - m.block_min_coord) / scale,
                    });
                }
            }
        }
        result
    }
}
