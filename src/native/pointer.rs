//! Route gestures to Markdown positions. A native input is a view of a source
//! range. Only a gesture wholly inside one input belongs to that input.
use super::{Field, Writing};
use blitz_dom::BaseDocument;
use dioxus_native::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Drag {
    field: Field,
    anchor: usize,
    across: bool,
}

#[derive(Clone, Copy)]
struct Hit {
    field: Field,
    pos: usize,
    input: bool,
}

fn axis_distance(point: f64, start: f64, size: f64) -> f64 {
    (start - point).max(point - start - size).max(0.0)
}

impl Writing {
    fn hit(self, x: f64, y: f64) -> Option<Hit> {
        let handle = self.root.peek().clone()?;
        // Resolve pending DOM mutations before hit-testing so Blitz's paint_children
        // never references a deleted NodeId.
        handle.doc_mut().resolve(0.0);
        let doc = handle.doc();
        let scroll = doc.viewport_scroll();
        let hit_node = doc
            .hit((x + scroll.x) as f32, (y + scroll.y) as f32)
            .map(|h| h.node_id)?;
        let mut target = hit_node;
        let mut input = None;
        let mut grid = None;
        loop {
            let node = doc.get_node(target)?;
            if let Some(el) = node.element_data() {
                if &*el.name.local == "button" {
                    return None;
                }
                if let Some(id) = el.id.as_deref() {
                    if id.starts_with("table-menu-") {
                        return None;
                    }
                    if el.text_input_data().is_some() {
                        input = Field::from_input_id(id);
                    }
                    if id == "paper" {
                        break;
                    }
                    // The frame belongs to the nearest cell. Only the actual
                    // scrollbar thumb takes the pointer away from the editor.
                    if id.starts_with("grid-scroll-")
                        && [doc.hovered_scrollbar(), doc.scrollbar_drag_target()]
                            .into_iter()
                            .flatten()
                            .any(|bar| bar.node_id == target)
                    {
                        return None;
                    }
                    if let Some(i) = id
                        .strip_prefix("grid-")
                        .and_then(|v| v.parse::<usize>().ok())
                    {
                        grid = Some(i);
                    }
                }
            }
            target = node.parent?;
        }
        let rect = |f: Field| doc.get_client_bounding_rect(doc.get_element_by_id(&f.id())?);
        // Choose a row by y first. Decorations in a table's two rails belong to
        // its edge cells. The blank line after the rail belongs to the document.
        let field = if let Some(field) = input {
            field
        } else if let Some(t) = grid {
            let table = self.tables.read();
            let row = (0..table[t].rows.len())
                .filter_map(|r| {
                    let b = rect(Field::Cell(t, r, 0))?;
                    Some((r, axis_distance(y, b.y, b.height)))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))?
                .0;
            self.column_at(&doc, t, row, x)?
        } else {
            let f = self
                .stops()
                .into_iter()
                .filter_map(|f| {
                    let b = rect(f)?;
                    Some((f, axis_distance(y, b.y, b.height)))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))?
                .0;
            if let Field::Cell(t, r, _) = f {
                self.column_at(&doc, t, r, x)?
            } else {
                f
            }
        };
        let id = doc.get_element_by_id(&field.id())?;
        let node = doc.get_node(id)?;
        let b = doc.get_client_bounding_rect(id)?;
        let l = node.final_layout();
        let text = node.element_data()?.text_input_data()?;
        let layout = text.editor.try_layout()?;
        let scale = f64::from(layout.scale());
        let (mut left, mut top) = (
            b.x + f64::from(l.padding.left + l.border.left),
            b.y + f64::from(l.padding.top + l.border.top),
        );
        if text.is_multiline {
            top -= f64::from(text.scroll_offset);
        } else {
            left -= f64::from(text.scroll_offset);
            top += node.text_input_v_centering_offset(scale);
        }
        let offset = parley::Cursor::from_point(
            layout,
            ((x - left) * scale) as f32,
            ((y - top) * scale) as f32,
        )
        .index();
        let proj = self.projection(field);
        let pos = proj.display_to_source(offset);
        Some(Hit {
            field,
            pos,
            input: input == Some(field),
        })
    }

    pub(super) fn column_at(self, doc: &BaseDocument, t: usize, r: usize, x: f64) -> Option<Field> {
        let viewport =
            doc.get_client_bounding_rect(doc.get_element_by_id(&format!("grid-scroll-{t}"))?)?;
        // Clip to the visible portion when a wide grid is horizontally scrolled.
        let x = x.clamp(
            viewport.x,
            (viewport.x + viewport.width - 0.01).max(viewport.x),
        );
        (0..self.tables.read()[t].width())
            .filter_map(|c| {
                let f = Field::Cell(t, r, c);
                let b = doc.get_client_bounding_rect(
                    doc.get_element_by_id(&format!("slot-{t}-{r}-{c}"))?,
                )?;
                Some((f, axis_distance(x, b.x, b.width)))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(f, _)| f)
    }

    pub(super) fn pointer_down(mut self, e: PointerEvent) {
        self.end_drag();
        self.pressed_box.set(None);
        if e.trigger_button() == Some(dioxus_html::input_data::MouseButton::Secondary) {
            let p = e.client_coordinates();
            if let Some(hit) = self.hit(p.x, p.y)
                && let Field::Cell(t, r, c) = hit.field
            {
                e.prevent_default();
                e.stop_propagation();
                let inside_rect = self.cells.peek().is_some_and(|(idx, rect)| {
                    idx == t && rect.rows().contains(&r) && rect.cols().contains(&c)
                });
                if !inside_rect {
                    self.clear_cells();
                    self.open(hit.field, [hit.pos; 2]);
                }
                self.table_menu_top.set(None);
                self.table_menu.set(Some(t));
            }
            return;
        }
        if e.trigger_button() != Some(dioxus_html::input_data::MouseButton::Primary) {
            return;
        }
        let p = e.client_coordinates();
        let Some(hit) = self.hit(p.x, p.y) else {
            return;
        };
        self.table_menu.set(None);
        self.table_menu_top.set(None);
        self.quiet_cell.set(None);
        let previous = *self.active.peek();
        let prior_rect = *self.cells.peek();
        if e.modifiers().shift()
            && let (Some(Field::Cell(t, r, c)), Field::Cell(other, row, col)) =
                (previous, hit.field)
            && t == other
            && ([r, c] != [row, col] || prior_rect.is_some())
        {
            let anchor = prior_rect
                .filter(|(i, _)| *i == t)
                .map_or([r, c], |(_, s)| s.anchor);
            e.prevent_default();
            self.select_cells(
                t,
                super::table::CellRect {
                    anchor,
                    head: [row, col],
                },
            );
            return;
        }
        self.clear_cells();
        // A click toggles the box that was painted when the button went down.
        // Clicking already revealed brackets is ordinary caret positioning.
        if matches!(hit.field, Field::Body(_))
            && hit.input
            && let Some(root) = self.root.peek().as_ref()
        {
            let doc = root.doc();
            if let Some(rect) = doc
                .get_element_by_id(&hit.field.id())
                .and_then(|id| doc.get_client_bounding_rect(id))
            {
                let (x, y) = ((p.x - rect.x) as f32, (p.y - rect.y) as f32);
                let token = self
                    .boxes
                    .peek()
                    .iter()
                    .find_map(|b| b.visible_at(hit.field, x, y));
                self.pressed_box.set(token);
            }
        }
        let anchor = if e.modifiers().shift() {
            self.absolute_selection().map_or(hit.pos, |s| s[0])
        } else {
            hit.pos
        };
        self.drag.set(Some(Drag {
            field: hit.field,
            anchor,
            across: false,
        }));
        if hit.input && !e.modifiers().shift() {
            if self.span.peek().is_some() {
                self.span.set(None);
            }
            return; // Parley handles double clicks, word drags and platform caret affinity.
        }
        e.prevent_default();
        self.open(hit.field, [anchor, hit.pos]);
    }

    pub(super) fn pointer_move(mut self, e: PointerEvent) {
        let Some(mut drag) = *self.drag.peek() else {
            let p = e.client_coordinates();
            let hovered = self.box_at_point(p.x, p.y);
            if *self.hovered_box.peek() != hovered {
                self.hovered_box.set(hovered);
            }
            return;
        };
        if !e
            .held_buttons()
            .contains(dioxus_html::input_data::MouseButton::Primary)
        {
            self.end_drag();
            return;
        }
        let p = e.client_coordinates();
        let Some(hit) = self.hit(p.x, p.y) else {
            return;
        };
        if let (Field::Cell(t, r, c), Field::Cell(other, row, col)) = (drag.field, hit.field)
            && t == other
            && (r != row || c != col || self.cells.peek().is_some())
        {
            e.prevent_default();
            let rect = super::table::CellRect {
                anchor: [r, c],
                head: [row, col],
            };
            if *self.cells.peek() != Some((t, rect)) {
                self.select_cells(t, rect);
            }
            if !drag.across {
                drag.across = true;
                self.drag.set(Some(drag));
            }
            return;
        }
        if hit.field == drag.field && !drag.across && hit.input {
            return;
        }
        e.prevent_default();
        if !drag.across {
            drag.across = true;
            self.drag.set(Some(drag));
        }
        self.clear_cells();
        self.open(hit.field, [drag.anchor, hit.pos]);
    }

    fn box_at_point(self, client_x: f64, client_y: f64) -> Option<usize> {
        let root = self.root.peek().clone()?;
        let doc = root.doc();
        for b in self.boxes.peek().iter() {
            if b.revealed {
                continue;
            }
            let Some(rect) = doc
                .get_element_by_id(&b.field.id())
                .and_then(|id| doc.get_client_bounding_rect(id))
            else {
                continue;
            };
            let (x, y) = ((client_x - rect.x) as f32, (client_y - rect.y) as f32);
            if let Some(open) = b.visible_at(b.field, x, y) {
                return Some(open);
            }
        }
        None
    }

    pub(super) fn clear_hover(mut self) {
        if self.hovered_box.peek().is_some() {
            self.hovered_box.set(None);
        }
    }

    pub(super) fn end_drag(mut self) {
        if self.drag.peek().is_some() {
            self.drag.set(None);
            if let Some(field) = *self.active.peek() {
                self.track_field(field);
            }
        }
    }
}
