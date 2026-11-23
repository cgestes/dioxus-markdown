use super::{EditorButton, Field, ToolIcon, Writing, report};
use dioxus_native::prelude::*;
use margin_editor_core::format::{InlineSelection, Style};

impl Writing {
    fn format_position(self) -> Option<[i32; 2]> {
        let [anchor, head] = (*self.selected_text.peek())?;
        if self.cells.peek().is_some() || self.table_menu.peek().is_some() {
            return None;
        }
        let field = (*self.active.peek())?;
        if *self.quiet_cell.peek() == Some(field)
            || matches!(field, Field::TitleGap | Field::Source)
        {
            return None;
        }
        let handle = self.root.peek().clone()?;
        let doc = handle.doc();
        let id = doc.get_element_by_id(&field.id())?;
        if doc.get_focussed_node_id() != Some(id) && self.focus.peek().is_none() {
            return None;
        }
        let node = doc.get_node(id)?;
        let input = node.element_data()?.text_input_data()?;
        let layout = input.editor.try_layout()?;
        let proj = self.projection(field);
        let cursor = |at: usize| {
            let byte = proj.source_to_display(at);
            parley::Cursor::from_byte_index(
                layout,
                byte.min(input.editor.raw_text().len()),
                parley::Affinity::Downstream,
            )
            .geometry(layout, 0.0)
        };
        let a = cursor(anchor);
        let b = cursor(head);
        let scale = f64::from(layout.scale());
        let rect = doc.get_client_bounding_rect(id)?;
        let l = node.final_layout();
        let x = rect.x + f64::from(l.padding.left + l.border.left)
            - if input.is_multiline {
                0.
            } else {
                f64::from(input.scroll_offset)
            };
        let y = rect.y
            + f64::from(l.padding.top + l.border.top)
            + node.text_input_v_centering_offset(scale)
            - if input.is_multiline {
                f64::from(input.scroll_offset)
            } else {
                0.
            };
        let (w, h) = doc.viewport().logical_size();
        let top_y = a.y0.min(b.y0);
        let bot_y = a.y1.max(b.y1);
        let line_x = if (a.y0 - b.y0).abs() < 1.0 {
            (a.x0 + b.x0) / 2.
        } else if a.y0 < b.y0 {
            a.x0
        } else {
            b.x0
        };
        let header_bottom = doc
            .get_element_by_id("workspace-bar")
            .and_then(|n| doc.get_client_bounding_rect(n))
            .map_or(0., |r| r.y + r.height);
        let min_top = (header_bottom + 8.).max(8.);
        let mut above = y + top_y / scale - 46.;
        let mut bottom = y + bot_y / scale;
        if bottom < 0. || y + top_y / scale > f64::from(h) {
            return None;
        }
        // Avoid covering a preceding heading when selecting the top line of a body block.
        if matches!(field, Field::Body(_)) && above < rect.y - 6. && bottom + 48. <= f64::from(h) {
            above = min_top - 1.;
        }
        if let Field::Cell(t, ..) = field
            && let Some(table) = doc
                .get_element_by_id(&format!("grid-scroll-{t}"))
                .and_then(|id| doc.get_client_bounding_rect(id))
        {
            if table.y >= min_top + 46. {
                above = table.y - 46.;
            } else if table.y + table.height + 46. <= f64::from(h) {
                above = min_top - 1.;
                bottom = table.y + table.height;
            }
        }
        let bar_w = 190.;
        Some([
            (x + line_x / scale - bar_w / 2.)
                .clamp(8., (f64::from(w) - bar_w - 8.).max(8.))
                .round() as i32,
            (if above >= min_top { above } else { bottom + 8. })
                .clamp(min_top, (f64::from(h) - 46.).max(min_top))
                .round() as i32,
        ])
    }
    pub(super) fn refresh_format_position(mut self) {
        let pos = self.format_position();
        if *self.format_position.peek() != pos {
            self.format_position.set(pos);
        }
    }
    pub(super) fn format(mut self, style: Style) -> bool {
        if self.cells.peek().is_some() {
            return false;
        }
        let Some(range) = (*self.selected_text.peek()).or_else(|| self.absolute_selection()) else {
            return false;
        };
        let md = self.contents.peek().clone();
        let Some(selection) = InlineSelection::read(&md, range) else {
            return false;
        };
        let edit = selection.toggle(&md, style);
        self.note.peek().remember_selection(range);
        let result = self.note.write().splice(&edit.changes);
        if result.is_ok() {
            let field = self.field_at(edit.selection[1]);
            self.open(field, edit.selection);
        }
        report(result, self.message);
        true
    }
}

#[component]
pub(super) fn FormatBar(writing: Writing) -> Element {
    let tips = if cfg!(target_os = "macos") {
        [
            "Bold (⌘B)",
            "Italic (⌘I)",
            "Strikethrough (⌘⇧X)",
            "Inline code (⌘E)",
            "Link (⌘K)",
        ]
    } else {
        [
            "Bold (Ctrl+B)",
            "Italic (Ctrl+I)",
            "Strikethrough (Ctrl+Shift+X)",
            "Inline code (Ctrl+E)",
            "Link (Ctrl+K)",
        ]
    };
    let styles = use_memo(move || {
        let range = (writing.selected_text)()?;
        if writing.cells.read().is_some() || writing.table_menu.read().is_some() {
            return None;
        }
        let selection = InlineSelection::read(&writing.contents.read(), range)?;
        Some(Style::ALL.map(|style| selection.active(style)))
    });
    let (Some(active), Some([x, y])) = (styles(), (writing.format_position)()) else {
        return rsx! {};
    };
    rsx! {
        div { id:"format-bar", class:"format-bar", role:"toolbar", aria_label:"Format selection",
            style:"left:{x}px; top:{y}px;",
            onpointerdown:move |e| e.prevent_default(),
            for (i,style) in Style::ALL.into_iter().enumerate() {
                EditorButton { key:"{i}", id:format!("format-{i}"), label:style.label(), tip:tips[i], icon:[ToolIcon::Bold,ToolIcon::Italic,ToolIcon::Strike,ToolIcon::Code,ToolIcon::Link][i], selected:active[i],
                    onactivate:move |_| { writing.format(style); } }
            }
        }
    }
}
