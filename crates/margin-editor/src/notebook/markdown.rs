//! Presentation ranges live in the same UTF-8 source coordinate space as Parley and Loro.
use super::ui::{Button, Icon, Menu, MenuItem, Variant, icon};
use super::{Field, Writing, report};
use crate::prelude::*;
use margin_editor_core::callout::CalloutKind;
use margin_projection::style::*;
use crate::platform::text::{self, Cluster};
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CaretInk {
    pub field: Field,
    x: f32,
    y: f32,
    height: f32,
}

#[component]
pub fn CaretOverlay(writing: Writing, field: Field) -> Element {
    let Some(caret) = (writing.caret)() else {
        return rsx! {};
    };
    if caret.field != field {
        return rsx! {};
    }
    rsx! {
        span {
            id: "caret-ink",
            class: "caret-ink",
            aria_hidden: "true",
            style: format!("left:{:.2}px; top:{:.2}px; height:{:.2}px;", caret.x, caret.y, caret.height),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CalloutBox {
    pub field: Field,
    pub start: usize,
    pub kind: CalloutKind,
    pub top: f32,
    pub height: f32,
    pub badge_x: f32,
    pub badge_y: f32,
    pub badge_w: f32,
    pub badge_h: f32,
    pub revealed: bool,
}

impl CalloutBox {
    pub fn draw(self) -> Element {
        let Self {
            start,
            kind,
            top,
            height,
            ..
        } = self;
        rsx! {
            div {
                key: "cb-{start}",
                id: "callout-{start}",
                class: "callout-bg {kind.css_class()}",
                aria_hidden: "true",
                style: format!("top:{top:.1}px; height:{height:.1}px;"),
            }
        }
    }

    pub fn draw_bar(self) -> Element {
        let Self {
            start,
            kind,
            top,
            height,
            ..
        } = self;
        rsx! {
            span {
                key: "cbar-{start}",
                class: "callout-bar {kind.css_class()}",
                aria_hidden: "true",
                style: format!("top:{top:.1}px; height:{height:.1}px;"),
            }
        }
    }

    pub fn draw_badge(self, writing: Writing) -> Element {
        let Self {
            start,
            kind,
            badge_x,
            badge_y,
            badge_w,
            badge_h,
            ..
        } = self;
        let open = *writing.callout_menu.read() == Some(start);
        rsx! {
            div {
                key: "cbadge-{start}",
                class: "callout-badge-anchor",
                class: if open { "menu-open" },
                style: format!("left:{badge_x:.1}px; top:{badge_y:.1}px;"),
                button {
                    id: "callout-badge-{start}",
                    class: "callout-badge-btn {kind.css_class()}",
                    class: if open { "open" },
                    aria_label: format!("Callout type: {}", kind.label()),
                    aria_expanded: if open { "true" } else { "false" },
                    title: "Change callout type",
                    style: format!("min-width:{badge_w:.1}px; height:{badge_h:.1}px;"),
                    onpointerdown: move |e| {
                        e.prevent_default();
                        e.stop_propagation();
                    },
                    onclick: move |e| {
                        e.stop_propagation();
                        let next = (*writing.callout_menu.peek() != Some(start)).then_some(start);
                        writing.show_callout_menu(next);
                    },
                    span { class: "callout-tag-inner",
                        {icon(Icon::for_callout(kind))}
                        span { "{kind.label()}" }
                    }
                }
                if open {
                    Menu {
                        id: "callout-menu",
                        class: "callout-menu",
                        label: "Callout type",
                        style: format!("left:0px; top:{:.1}px;", badge_h + 2.0),
                        for option in CalloutKind::ALL {
                            MenuItem {
                                key: "{option.tag()}",
                                id: format!("callout-pick-{}", option.tag().to_ascii_lowercase()),
                                label: option.label().to_owned(),
                                icon: Icon::for_callout(option),
                                selected: option == kind,
                                onclick: move |_| writing.set_callout_kind(start, option),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct CodeBox {
    pub field: Field,
    pub start: usize,
    pub lang: String,
    pub body_range: Range<usize>,
    pub top: f32,
    pub height: f32,
}

impl CodeBox {
    pub fn draw(self) -> Element {
        let Self {
            start, top, height, ..
        } = self;
        rsx! {
            div {
                key: "code-bg-{start}",
                id: "code-block-{start}",
                class: "code-bg",
                aria_hidden: "true",
                style: format!("top:{top:.1}px; height:{height:.1}px;"),
            }
        }
    }

    pub fn draw_tools(self, writing: Writing) -> Element {
        let Self {
            start,
            lang,
            body_range,
            top,
            ..
        } = self;
        rsx! {
            CodeBlockTools {
                key: "code-tools-{start}",
                writing,
                start,
                lang,
                body_range,
                top: top + 5.0,
            }
        }
    }
}

#[component]
fn CodeBlockTools(
    writing: Writing,
    start: usize,
    lang: String,
    body_range: Range<usize>,
    top: f32,
) -> Element {
    let mut copied = use_signal(|| false);
    rsx! {
        div {
            class: "code-tools",
            style: format!("top:{top:.1}px;"),
            onpointerdown: move |e| {
                e.prevent_default();
                e.stop_propagation();
            },
            if !lang.is_empty() {
                span { class: "code-lang", "{lang}" }
            }
            Button {
                id: format!("code-copy-{start}"),
                variant: Variant::Icon,
                label: "Copy code",
                tip: if copied() { "Copied" } else { "Copy code" },
                icon: if copied() { Icon::Check } else { Icon::Copy },
                selected: copied().then_some(true),
                keep_focus: true,
                onclick: move |_| {
                    let md = writing.contents.read();
                    if let Some(snippet) = md.get(body_range.clone())
                        && let Some(handle) = writing.root.peek().as_ref()
                        && handle
                            .doc()
                            .shell_provider
                            .set_clipboard_text(snippet.to_owned())
                            .is_ok()
                    {
                        copied.set(true);
                    }
                },
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct Ink {
    pub field: Field,
    at: usize,
    style: u32,
    text: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// Letter spacing that makes the painted run exactly as wide as the
    /// regular text beneath it (Rich mode bold and italic).
    letter: f32,
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
            letter,
            ..
        } = self;
        let callout = callout_kind(style);
        let callout_cls = callout.map_or("", CalloutKind::css_class);
        let is_callout_quote_mark = callout.is_some() && style & QUOTE_MARK != 0;
        let is_callout_title = callout.is_some() && style & CALLOUT_TITLE != 0;
        rsx! {
            span {
                key: "{at}", id: "ink-{at}", class: "md-ink {callout_cls}", aria_hidden: "true",
                class: if style & (STRONG | HEADING) != 0 { "strong" },
                class: if style & ITALIC != 0 { "emphasis" },
                class: if style & CODE != 0 { "code" },
                class: if style & CODE_BLOCK != 0 { "code-block" },
                class: if style & LINK != 0 { "link" },
                class: if style & HIGHLIGHT != 0 { "highlight" },
                class: if style & STRIKE != 0 { "strike" },
                class: if style & QUOTE != 0 { "quote" },
                class: if style & QUOTE_MARK != 0 && !is_callout_quote_mark { "quote-mark" },
                class: if is_callout_quote_mark { "callout-quote-mark" },
                class: if is_callout_title { "callout-title-ink" },
                class: if style & RULE != 0 { "rule" },
                class: if style & PUNCTUATION != 0 { "punctuation" },
                class: if style & DONE != 0 { "done" },
                style: if letter != 0.0 {
                    format!("left:{x}px; top:{y}px; width:{width}px; height:{height}px; line-height:{height}px; letter-spacing:{letter:.3}px;")
                } else {
                    format!("left:{x}px; top:{y}px; width:{width}px; height:{height}px; line-height:{height}px;")
                },
                if is_callout_quote_mark {
                    ""
                } else if style & BULLET != 0 {
                    "•"
                } else if style & QUOTE_MARK != 0 {
                    span { class: "quote-bar" }
                } else if style & RULE != 0 {
                    span { class: "rule-line" }
                } else {
                    "{text}"
                }
            }
        }
    }
}


impl Writing {
    pub fn inline_fields(self) -> Vec<Field> {
        let mut fields = Vec::new();
        for start in self.parsed.read().inline_token_starts(INLINE) {
            let field = self.field_at(start);
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
        let proj = self.projection_peek(field);
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
        // Rich mode sets prose in a proportional face: a bold or italic run is
        // wider or narrower than the regular glyphs it covers.
        let rich = *self.edit_mode.peek() == super::projection::EditMode::Rich;
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
                    let reveal_syntax = token.style == PUNCTUATION
                        || (!rich
                            && token.style & CALLOUT_ANY != 0
                            && token.style & QUOTE_MARK != 0);
                    if slice.is_empty()
                        || slice.contains('\t')
                        || (reveal_syntax
                            && node.is_focussed()
                            && !whole_cell
                            && selection.start <= start + slice.len()
                            && selection.end >= start)
                        || (!reveal_syntax
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
                    let fitted = rich
                        && token.style & (STRONG | ITALIC) != 0
                        && token.style & (HEADING | CODE | PUNCTUATION) == 0;
                    let letter = fitted
                        .then(|| {
                            let weight = if token.style & STRONG != 0 { 600.0 } else { 400.0 };
                            let natural = crate::typefaces::prose_width(slice, run.font_size(), weight, token.style & ITALIC != 0)?;
                            let chars = slice.chars().count().max(1) as f32;
                            Some((width * scale - natural) / chars / scale)
                        })
                        .flatten()
                        .unwrap_or(0.0);
                    result.push(Ink {
                        field,
                        at: token.source_start + (start - token.display.start),
                        style: token.style,
                        text: slice.to_owned(),
                        x: left_inset + x / scale,
                        y: top_inset + (m.block_min_coord - top) / scale,
                        width,
                        height: (m.block_max_coord - m.block_min_coord) / scale,
                        letter,
                    });
                }
            }
        }
        result
    }

    fn vertical_band(
        layout: &text::Layout,
        proj: &super::projection::Projection,
        raw_len: usize,
        range: &Range<usize>,
        scroll_top: f32,
        scale: f32,
        top_inset: f32,
        pad_y: f32,
    ) -> (f32, f32) {
        let d_start = proj.source_to_display(range.start).min(raw_len);
        let d_end = proj
            .source_to_display(range.end.saturating_sub(1).max(range.start))
            .min(raw_len);
        let first_geom =
            text::Cursor::from_byte_index(layout, d_start, text::Affinity::Downstream)
                .geometry(layout, 0.0);
        let last_geom =
            text::Cursor::from_byte_index(layout, d_end, text::Affinity::Downstream)
                .geometry(layout, 0.0);
        let min_y = (first_geom.y0.min(last_geom.y0) as f32 - scroll_top) / scale;
        let max_y = (first_geom.y1.max(last_geom.y1) as f32 - scroll_top) / scale;
        (
            top_inset + min_y - pad_y,
            (max_y - min_y + pad_y * 2.0).max(28.0),
        )
    }

    pub fn measured_prose_boxes(self, field: Field) -> (Vec<CalloutBox>, Vec<CodeBox>) {
        let parsed = self.parsed.read();
        if parsed.callouts.is_empty() && parsed.code_blocks.is_empty() {
            return (Vec::new(), Vec::new());
        }
        let handle = self.root.peek();
        let Some(handle) = handle.as_ref() else {
            return (Vec::new(), Vec::new());
        };
        let doc = handle.doc();
        let Some(node) = doc
            .get_element_by_id(&field.id())
            .and_then(|id| doc.get_node(id))
        else {
            return (Vec::new(), Vec::new());
        };
        let Some(input) = node.element_data().and_then(|e| e.text_input_data()) else {
            return (Vec::new(), Vec::new());
        };
        let Some(layout) = input.editor.try_layout() else {
            return (Vec::new(), Vec::new());
        };
        let proj = self.projection_peek(field);
        if input.editor.is_composing() || input.editor.raw_text() != proj.display {
            return (Vec::new(), Vec::new());
        }
        let md = self.contents.read();
        let field_range = self.range(field, &md);
        let rich = *self.edit_mode.peek() == super::projection::EditMode::Rich;
        let selection = input.editor.raw_selection().text_range();
        let scale = layout.scale();
        let scroll_top = if input.is_multiline {
            input.scroll_offset * scale
        } else {
            0.0
        };
        let l = node.final_layout();
        let left_inset = l.border.left + l.padding.left;
        let top_inset = l.border.top + l.padding.top;
        let raw_len = input.editor.raw_text().len();

        let callouts = parsed
            .callouts
            .iter()
            .filter(|c| c.range.start >= field_range.start && c.range.end <= field_range.end)
            .map(|c| {
                let (top, height) = Self::vertical_band(
                    layout,
                    &proj,
                    raw_len,
                    &c.range,
                    scroll_top,
                    scale,
                    top_inset,
                    7.0,
                );
                let tag_d_start = proj.source_to_display(c.tag_range.start).min(raw_len);
                let tag_d_end = proj.source_to_display(c.tag_range.end).min(raw_len);
                let tag_start_geom = text::Cursor::from_byte_index(
                    layout,
                    tag_d_start,
                    text::Affinity::Downstream,
                )
                .geometry(layout, 0.0);
                let tag_end_geom = text::Cursor::from_byte_index(
                    layout,
                    tag_d_end,
                    text::Affinity::Downstream,
                )
                .geometry(layout, 0.0);
                CalloutBox {
                    field,
                    start: c.range.start,
                    kind: c.kind,
                    top,
                    height,
                    badge_x: left_inset + tag_start_geom.x0 as f32 / scale,
                    badge_y: top_inset + (tag_start_geom.y0 as f32 - scroll_top) / scale,
                    badge_w: ((tag_end_geom.x0 - tag_start_geom.x0) as f32 / scale).max(24.0),
                    badge_h: ((tag_start_geom.y1 - tag_start_geom.y0) as f32 / scale).max(20.0),
                    revealed: !rich
                        && node.is_focussed()
                        && selection.start <= tag_d_end
                        && selection.end >= tag_d_start,
                }
            })
            .collect();

        let code_blocks = parsed
            .code_blocks
            .iter()
            .filter(|cb| cb.range.start >= field_range.start && cb.range.end <= field_range.end)
            .map(|cb| {
                let (top, height) = Self::vertical_band(
                    layout,
                    &proj,
                    raw_len,
                    &cb.range,
                    scroll_top,
                    scale,
                    top_inset,
                    6.0,
                );
                CodeBox {
                    field,
                    start: cb.range.start,
                    lang: cb.lang.clone(),
                    body_range: cb.body_range.clone(),
                    top,
                    height,
                }
            })
            .collect();

        (callouts, code_blocks)
    }

    pub fn set_callout_kind(mut self, callout_start: usize, kind: CalloutKind) {
        self.callout_menu.set(None);
        let tag_range = self
            .parsed
            .read()
            .callouts
            .iter()
            .find(|c| c.range.start == callout_start)
            .map(|c| c.tag_range.clone());
        let Some(range) = tag_range else {
            return;
        };
        let replacement = format!("[!{}]", kind.tag());
        if let Some(sel) = self.absolute_selection() {
            self.note.peek().remember_selection(sel);
        }
        let result = self.note.write().replace(range.clone(), &replacement);
        if result.is_ok() {
            let caret = range.start + replacement.len();
            self.open(self.field_at(caret), [caret; 2]);
        }
        report(result, self.message);
    }

    pub fn measured_caret(self) -> Option<CaretInk> {
        if self.span.peek().is_some() || self.cells.peek().is_some() {
            return None;
        }
        let field = (*self.active.peek())?;
        if matches!(field, Field::Source | Field::TitleGap) {
            return None;
        }
        let handle = self.root.peek().clone()?;
        let doc = handle.doc();
        let id = doc.get_element_by_id(&field.id())?;
        if doc.get_focussed_node_id() != Some(id) {
            return None;
        }
        let node = doc.get_node(id)?;
        let input = node.element_data()?.text_input_data()?;
        if !input.editor.raw_selection().is_collapsed() {
            return None;
        }
        let layout = input.editor.try_layout()?;
        let cursor = input.editor.cursor_geometry(1.5)?;
        let scale = layout.scale();
        let l = node.final_layout();
        let top = if input.is_multiline {
            input.scroll_offset * scale
        } else {
            0.0
        };
        let left_inset = l.border.left + l.padding.left
            - if input.is_multiline {
                0.0
            } else {
                input.scroll_offset
            };
        let top_inset = l.border.top
            + l.padding.top
            + node.text_input_v_centering_offset(f64::from(scale)) as f32;
        let x = left_inset + cursor.x0 as f32 / scale;
        let y = top_inset + (cursor.y0 as f32 - top) / scale;
        let height = ((cursor.y1 - cursor.y0) as f32 / scale).max(12.0);
        let max_x = l.content_box_width() + l.padding.left + l.padding.right;
        let max_y = l.content_box_height() + l.padding.top + l.padding.bottom;
        if y + height < 0.0 || y > max_y || x + 1.5 < 0.0 || x > max_x {
            return None;
        }
        let x = if !input.is_multiline {
            x.clamp(0.0, (max_x - 1.5).max(0.0))
        } else {
            x
        };
        Some(CaretInk {
            field,
            x,
            y,
            height,
        })
    }
}
