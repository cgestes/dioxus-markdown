use super::Writing;
use super::{Field, report};
use crate::prelude::*;
use margin_editor_core::checkbox::{Input, Mode, markdown_markers};
use crate::platform::text::Cluster;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct MarkerBox {
    pub field: Field,
    pub open: usize,
    checked: bool,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    pub revealed: bool,
    selected: bool,
}

impl MarkerBox {
    pub(super) fn visible_at(&self, field: Field, x: f32, y: f32) -> Option<usize> {
        (self.field == field
            && !self.revealed
            && (self.left..self.left + self.width).contains(&x)
            && (self.top..self.top + self.height).contains(&y))
        .then_some(self.open)
    }

    pub fn draw(self, writing: Writing) -> Element {
        let Self {
            field,
            open,
            checked,
            left,
            top,
            width,
            height,
            selected,
            ..
        } = self;
        rsx! {
            div {
                key: "{open}", id: "checkbox-{open}", class: "marker-slot", role: "checkbox",
                aria_checked: checked, aria_label: "Checkbox",
                style: "left:{left}px; top:{top}px; width:{width}px; height:{height}px;",
                onclick: move |e| {
                    e.stop_propagation();
                    writing.click_checkbox(field, open, e.modifiers());
                },
                // Where no click follows the release, the release toggles.
                // click_checkbox consumes the press, so a click that does arrive
                // afterwards is ignored.
                onpointerup: move |e| {
                    if !crate::platform::click_follows_release(&e) {
                        writing.end_drag();
                        writing.click_checkbox(field, open, e.modifiers());
                    }
                },
                span { class: "marker-face", class: if checked { "checked" }, class: if selected { "selected" } }
                if checked { span { class: "marker-tick" } }
            }
        }
    }
}

impl Writing {
    // Paint over the original shaped glyphs. Removing one overlay exposes the real
    // brackets, caret, IME and selection without changing native text layout.
    fn measured_boxes(self, field: Field) -> Vec<MarkerBox> {
        let root = self.root.peek();
        let Some(root) = root.as_ref() else {
            return Vec::new();
        };
        let doc = root.doc();
        let Some(node) = doc
            .get_element_by_id(&field.id())
            .and_then(|id| doc.get_node(id))
        else {
            return Vec::new();
        };
        let Some(input) = node.element_data().and_then(|e| e.text_input_data()) else {
            return Vec::new();
        };
        let edit = &input.editor;
        let Some(layout) = edit.try_layout() else {
            return Vec::new();
        };
        let md = self.contents.read();
        let proj = self.projection_peek(field);
        let selection = edit.raw_selection().text_range();
        let markers = if edit.is_composing() {
            let body = self.range(field, &md).start;
            let composing = markdown_markers(&format!("{}{}", &md[..body], edit.raw_text()));
            return composing
                .into_iter()
                .filter_map(|token| {
                    let open = token.open.checked_sub(body)?;
                    let first = Cluster::from_byte_index(layout, open)?;
                    let last = Cluster::from_byte_index(layout, open + 2)?;
                    if first.line().text_range() != last.line().text_range() {
                        return None;
                    }
                    let a = first.visual_offset()?;
                    let b = last.visual_offset()?;
                    let left = a.min(b);
                    let right = (a + first.advance()).max(b + last.advance());
                    let scale = layout.scale();
                    let top = input.scroll_offset * scale;
                    let line = first.line();
                    let metrics = line.metrics();
                    Some(MarkerBox {
                        field,
                        open: token.open,
                        checked: token.checked,
                        left: left / scale,
                        top: (metrics.block_min_coord - top) / scale,
                        width: (right - left) / scale,
                        height: (metrics.block_max_coord - metrics.block_min_coord) / scale,
                        revealed: false,
                        selected: false,
                    })
                })
                .collect();
        } else {
            proj.checkboxes
        };
        if edit.raw_text() != proj.display {
            return Vec::new();
        }
        let scale = layout.scale();
        let top = input.scroll_offset * scale;
        let bottom = top + node.final_layout().content_box_height() * scale;
        let mut visible = layout.lines().filter(|l| {
            l.metrics().block_max_coord >= top && l.metrics().block_min_coord <= bottom
        });
        let Some(first) = visible.next() else {
            return Vec::new();
        };
        let first_byte = first.text_range().start;
        let last_byte = visible
            .next_back()
            .map_or(first.text_range().end, |l| l.text_range().end);
        markers
            .into_iter()
            .filter(|m| !m.revealed && m.display.end > first_byte && m.display.start < last_byte)
            .filter_map(|token| {
                let start = token.display.start;
                let end = token.display.end.checked_sub(1)?;
                let first = Cluster::from_byte_index(layout, start)?;
                let last = Cluster::from_byte_index(layout, end)?;
                if first.line().text_range() != last.line().text_range() {
                    return None;
                }
                let a = first.visual_offset()?;
                let b = last.visual_offset()?;
                let left = a.min(b);
                let right = (a + first.advance()).max(b + last.advance());
                let line = first.line();
                let metrics = line.metrics();
                let left_px = (left / scale).round();
                let right_px = (right / scale).floor();
                let width_px = (right_px - left_px).max(15.0);
                Some(MarkerBox {
                    field,
                    open: token.open,
                    checked: token.checked,
                    left: left_px,
                    top: (metrics.block_min_coord - top) / scale,
                    width: width_px,
                    height: (metrics.block_max_coord - metrics.block_min_coord) / scale,
                    revealed: token.revealed,
                    selected: node.is_focussed()
                        && !selection.is_empty()
                        && selection.start < token.display.end
                        && selection.end > token.display.start,
                })
            })
            .collect()
    }

    pub fn refresh_boxes(mut self) {
        let count = self.blocks.read().len();
        let ink = (0..=count)
            .map(Field::Body)
            .chain(self.inline_fields())
            .flat_map(|f| self.measured_ink(f))
            .collect::<Vec<_>>();
        if *self.ink.peek() != ink {
            self.ink.set(ink);
        }
        let mut callout_boxes = Vec::new();
        let mut code_boxes = Vec::new();
        for i in 0..=count {
            let (callouts, codes) = self.measured_prose_boxes(Field::Body(i));
            callout_boxes.extend(callouts);
            code_boxes.extend(codes);
        }
        if *self.callout_boxes.peek() != callout_boxes {
            self.callout_boxes.set(callout_boxes);
        }
        if *self.code_boxes.peek() != code_boxes {
            self.code_boxes.set(code_boxes);
        }
        let next = (0..=count)
            .flat_map(|i| self.measured_boxes(Field::Body(i)))
            .collect::<Vec<_>>();
        if self.drag.peek().is_none()
            && let Some(root) = self.root.peek().as_ref()
        {
            let doc = root.doc();
            let measure = |field: Field| -> usize {
                let Some(node) = doc
                    .get_element_by_id(&field.id())
                    .and_then(|id| doc.get_node(id))
                else {
                    return 0;
                };
                let Some(layout) = node
                    .element_data()
                    .and_then(|e| e.text_input_data())
                    .and_then(|t| t.editor.try_layout())
                else {
                    return 0;
                };
                (layout.height() / layout.scale()).ceil() as usize
            };
            let heights = (0..=count)
                .map(|i| measure(Field::Body(i)))
                .collect::<Vec<_>>();
            if *self.heights.peek() != heights {
                self.heights.set(heights);
            }
            let heading_heights = std::iter::once(measure(Field::Title))
                .chain((0..count).map(|i| measure(Field::Heading(i))))
                .collect::<Vec<_>>();
            if *self.heading_heights.peek() != heading_heights {
                self.heading_heights.set(heading_heights);
            }
            let row_heights = self
                .tables
                .peek()
                .iter()
                .enumerate()
                .map(|(t, grid)| {
                    (0..grid.rows.len())
                        .map(|r| {
                            doc.get_element_by_id(&format!("slot-{t}-{r}-0"))
                                .and_then(|id| doc.get_client_bounding_rect(id))
                                .map_or(0.0, |rect| rect.height)
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            if *self.grid.row_heights.peek() != row_heights {
                self.grid.row_heights.set(row_heights);
            }
        }
        if *self.boxes.peek() != next {
            self.boxes.set(next);
        }
        let caret = self.measured_caret();
        if *self.caret.peek() != caret {
            self.caret.set(caret);
        }
    }

    pub fn click_checkbox(mut self, field: Field, open: usize, modifiers: Modifiers) {
        if modifiers.intersects(Modifiers::SHIFT | Modifiers::META | Modifiers::CONTROL) {
            self.pressed_box.set(None);
            self.track_field(field);
            return;
        }
        let pressed = self.pressed_box.take();
        if pressed != Some(open) {
            return;
        }
        if self.literal.peek().is_some() {
            self.track_field(field);
            return;
        }
        let inside = open + 1;
        if modifiers.contains(Modifiers::ALT) {
            // Alt+click reveals the Markdown brackets for direct editing.
            self.mouse_box.set(None);
            self.open(field, [inside; 2]);
            return;
        }
        // Ordinary mouse click toggles the box in place and keeps it rendered as a togglebox!
        self.mouse_box.set(Some(open));
        report(
            self.note
                .write()
                .edit_marker(inside..inside, Mode::Structured, Input::Type(' ')),
            self.message,
        );
        self.open(field, [open + 3; 2]);
    }

    pub fn click_body(mut self, field: Field, event: MouseEvent) {
        if event
            .modifiers()
            .intersects(Modifiers::SHIFT | Modifiers::META | Modifiers::CONTROL)
        {
            self.track_field(field);
            return;
        }
        let Some(open) = self.pressed_box.take() else {
            self.mouse_box.set(None);
            self.track_field(field);
            return;
        };
        if self.literal.peek().is_some() {
            self.track_field(field);
            return;
        }
        let inside = open + 1;
        if event.modifiers().contains(Modifiers::ALT) {
            // Alt+click reveals the Markdown brackets for direct editing.
            self.mouse_box.set(None);
            self.open(field, [inside; 2]);
            return;
        }
        // Ordinary mouse click toggles the box in place and keeps it rendered as a togglebox!
        self.mouse_box.set(Some(open));
        report(
            self.note
                .write()
                .edit_marker(inside..inside, Mode::Structured, Input::Type(' ')),
            self.message,
        );
        self.open(field, [open + 3; 2]);
    }
}

#[component]
pub(super) fn WindowLayout(writing: Writing) -> Element {
    use crate::platform::FieldEvent;
    let mut writing = writing;
    // The platform reports when the focused field's caret, text or geometry
    // has settled; the overlays are then measured against the new layout.
    crate::platform::use_field_watch(writing.root, move |event| match event {
        FieldEvent::Composing => {
            if writing.grid.cells.peek().is_some() {
                writing.grid.editing.set(true);
            }
        }
        FieldEvent::ViewportChanged => {
            if writing.code_lang_menu.peek().is_some() {
                writing.code_lang_menu.set(None);
            }
        }
        FieldEvent::Changed => {
            if writing.drag.peek().is_none() {
                if writing.focus.peek().is_none()
                    && let Some(field) = *writing.active.peek()
                {
                    writing.track_field(field);
                }
                writing.refresh_format_position();
                writing.refresh_empty_line();
                writing.refresh_slash();
                writing.refresh_link_card();
            }
            writing.refresh_boxes();
        }
    });
    rsx! {}
}
