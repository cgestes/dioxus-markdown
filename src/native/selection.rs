//! The focused input projects part of the Markdown selection. Its anchor and
//! moving end may be on opposite sides of a widget or hidden syntax.
use super::{ByteSelection, Field, Sections, Writing, document::BodyStart, report};
use dioxus_native::prelude::*;

impl Writing {
    pub fn absolute_selection(self) -> Option<ByteSelection> {
        if let Some(span) = *self.span.peek() {
            return Some(span);
        }
        let field = (*self.active.peek())?;
        let proj = self.projection_peek(field);
        let root = self.root.peek();
        let doc = root.as_ref()?.doc();
        let id = doc.get_element_by_id(&field.id())?;
        let editor = &doc.get_node(id)?.element_data()?.text_input_data()?.editor;
        if editor.raw_text() != proj.display {
            return (*self.cursor_source.peek()).or_else(|| {
                let selected = editor.raw_selection();
                Some([
                    proj.display_to_source(selected.anchor().index()),
                    proj.display_to_source(selected.focus().index()),
                ])
            });
        }
        let selected = editor.raw_selection();
        Some([
            proj.display_to_source(selected.anchor().index()),
            proj.display_to_source(selected.focus().index()),
        ])
    }

    pub fn open(mut self, field: Field, absolute: ByteSelection) {
        if self.quiet_cell.peek().is_some() {
            self.quiet_cell.set(None);
        }
        self.keep_cells_in(field);
        if self.parked_selection.peek().is_some() {
            self.parked_selection.set(None);
        }
        let md = self.contents.read();
        let next_body = if matches!(field, Field::Body(_)) {
            let parts = Sections::read(&md);
            Some(
                if parts.origin == BodyStart::AfterBlankLine && absolute[1] < parts.body {
                    BodyStart::AfterHeading
                } else {
                    parts.origin
                },
            )
        } else {
            None
        };
        if *self.body.peek() != next_body {
            self.body.set(next_body);
        }
        if self.literal.peek().is_some() {
            self.literal.set(None);
        }
        if *self.active.peek() != Some(field) {
            self.active.set(Some(field));
        }
        // Keep inline delimiter projection steady under a moving pointer during drags;
        // `end_drag` settles `cursor_source` once when the pointer is released.
        if self.drag.peek().is_none() && *self.cursor_source.peek() != Some(absolute) {
            self.cursor_source.set(Some(absolute));
        }
        let range = self.range(field, &md);
        let cross = absolute[0] != absolute[1]
            && absolute
                .iter()
                .any(|i| !((range.start..=range.end).contains(i)));
        let next_span = cross.then_some(absolute);
        if *self.span.peek() != next_span {
            self.span.set(next_span);
        }
        let selected = if cross { [absolute[1]; 2] } else { absolute };
        drop(md);
        let proj = self.projection_peek(field);
        let local = selected.map(|i| proj.source_to_display(i));
        self.focus.set(Some((field, local)));
        *self.last_tracked.write_unchecked() = None;
        self.note.peek().remember_selection(absolute);
        self.track(absolute);
    }

    pub(super) fn history(mut self, source: bool, redo: bool) {
        self.clear_cells();
        self.table_menu.set(None);
        let selected = if source {
            super::selection(self.root, Field::Source)
        } else {
            self.absolute_selection()
        };
        if let Some(s) = selected {
            self.note.peek().remember_selection(s);
        }
        let result = if redo {
            self.note.write().redo()
        } else {
            self.note.write().undo()
        };
        if matches!(result, Ok(true)) {
            let md = self.contents.read().clone();
            let range = self
                .note
                .peek()
                .take_history_selection()
                .or(selected)
                .unwrap_or([0; 2])
                .map(|p| md.floor_char_boundary(p.min(md.len())));
            if source {
                self.focus.set(Some((Field::Source, range)));
            } else {
                self.body.set(None);
                self.draft_table.set(None);
                self.open(self.field_at(range[1]), range);
            }
        }
        report(result, self.message);
    }
}

/// Keep the focused input's internal scroll offset aligned with its caret,
/// ensuring a full 2px CSS margin on the right edge of single-line inputs
/// (such as long table cells) so the 1.5px caret is never clipped at 1× or 2×.
pub(super) fn keep_caret_in_view(doc: &mut blitz_dom::BaseDocument, id: dioxus_native::NodeId) {
    let Some(node) = doc.get_node_mut(id) else {
        return;
    };
    let width = node.final_layout().content_box_width();
    let height = node.final_layout().content_box_height();
    let Some(input) = node
        .element_data_mut()
        .and_then(|e| e.text_input_data_mut())
    else {
        return;
    };
    input.clamp_scroll_offset(width, height);
    if !input.is_multiline
        && width > 0.0
        && let Some(layout) = input.editor.try_layout()
        && let Some(caret) = input.editor.cursor_geometry(1.5)
    {
        let scale = layout.scale();
        let caret_x = caret.x0 as f32 / scale;
        let caret_pad = 2.0_f32;
        let caret_end = caret_x + caret_pad;
        let content = layout.full_width() / scale;
        let max_offset = (content.max(caret_end) - width).max(0.0);
        let mut offset = input.scroll_offset;
        if caret_end > offset + width {
            offset = caret_end - width;
        }
        if caret_x < offset {
            offset = caret_x;
        }
        input.scroll_offset = offset.clamp(0.0, max_offset);
    }
}

/// An auto-sized prose field can be taller than the window. Scroll to the
/// shaped caret inside it so the next line stays visible at every zoom level.
pub(super) fn scroll_caret(doc: &mut blitz_dom::BaseDocument, id: dioxus_native::NodeId) {
    keep_caret_in_view(doc, id);
    let Some(node) = doc.get_node(id) else { return };
    let Some(input) = node.element_data().and_then(|e| e.text_input_data()) else {
        return;
    };
    let Some(layout) = input.editor.try_layout() else {
        return;
    };
    let Some(rect) = doc.get_client_bounding_rect(id) else {
        return;
    };
    let l = node.final_layout();
    let scale = f64::from(layout.scale());
    let origin = rect.y
        + f64::from(l.border.top + l.padding.top)
        + node.text_input_v_centering_offset(scale)
        - if input.is_multiline {
            f64::from(input.scroll_offset)
        } else {
            0.0
        };
    let caret = input.editor.raw_selection().focus().geometry(layout, 0.0);
    let top = origin + caret.y0 / scale;
    let bottom = origin + caret.y1 / scale;
    let height = f64::from(doc.viewport().logical_size().1);
    let margin = (height * 0.14).min(52.0);
    let dy = (top - margin).min(0.0) + (bottom - (height - margin)).max(0.0);
    if dy != 0.0
        && let Some(root) = doc.try_root_element().map(|n| n.id)
    {
        doc.scroll_by(root, 0.0, dy, blitz_dom::ScrollBehavior::Instant);
    }
}

// This is only painted when the selection spans native fields. Parley supplies
// the per-line, bidi-aware rectangles from exactly the layout used for typing.
#[component]
pub(super) fn SelectionInk(writing: Writing, field: Field) -> Element {
    let Some([anchor, head]) = (writing.span)() else {
        return rsx! {};
    };
    let md = writing.contents.read();
    let bounds = writing.range(field, &md);
    let start = anchor.min(head).max(bounds.start);
    let end = anchor.max(head).min(bounds.end);
    if start > end || start == end && !bounds.is_empty() {
        return rsx! {};
    }
    let root = writing.root.read();
    let Some(doc) = root.as_ref().map(|h| h.doc()) else {
        return rsx! {};
    };
    let Some(node) = doc
        .get_element_by_id(&field.id())
        .and_then(|id| doc.get_node(id))
    else {
        return rsx! {};
    };
    let Some(input) = node.element_data().and_then(|e| e.text_input_data()) else {
        return rsx! {};
    };
    let Some(layout) = input.editor.try_layout() else {
        return rsx! {};
    };
    let proj = writing.projection_peek(field);
    let cursor = |p| {
        parley::Cursor::from_byte_index(
            layout,
            proj.source_to_display(p),
            parley::Affinity::Downstream,
        )
    };
    let selected = parley::Selection::new(cursor(start), cursor(end));
    let l = node.final_layout();
    let scale = f64::from(layout.scale());
    let x = f64::from(
        l.padding.left + l.border.left
            - if input.is_multiline {
                0.0
            } else {
                input.scroll_offset
            },
    );
    let y = f64::from(
        l.padding.top + l.border.top
            - if input.is_multiline {
                input.scroll_offset
            } else {
                0.0
            },
    ) + node.text_input_v_centering_offset(scale);
    let rects = if start == end {
        vec![(cursor(start).geometry(layout, 3.0), 0)]
    } else {
        selected.geometry(layout)
    };
    rsx! { for (rect,_) in rects {
        span { class: "selection-ink", aria_hidden: "true",
            style: format!("left:{}px; top:{}px; width:{}px; height:{}px;",x+rect.x0/scale,y+rect.y0/scale,(rect.x1-rect.x0)/scale,(rect.y1-rect.y0)/scale) }
    } }
}
