//! Route gestures to Markdown positions. A native input is a view of a source
//! range. Only a gesture wholly inside one input belongs to that input.
use super::{Field, Writing};
use blitz_dom::BaseDocument;
use dioxus_native::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Drag {
    pub(super) field: Field,
    anchor: usize,
    last: [usize; 2],
    pub(super) across: bool,
    native: bool,
}

#[derive(Clone, Copy)]
struct Hit {
    field: Field,
    pos: usize,
    input: bool,
    checkbox: Option<usize>,
}

fn axis_distance(point: f64, start: f64, size: f64) -> f64 {
    (start - point).max(point - start - size).max(0.0)
}

impl Writing {
    fn hit(self, x: f64, y: f64) -> Option<Hit> {
        let handle = self.root.peek().clone()?;
        // Resolve pending DOM mutations before hit-testing so Blitz's paint_children
        // never references a deleted NodeId, without forcing layout when clean.
        self.resolve_dom(&handle);
        let doc = handle.doc();
        let scroll = doc.viewport_scroll();
        let hit_node = doc
            .hit((x + scroll.x) as f32, (y + scroll.y) as f32)
            .map(|h| h.node_id)?;
        let mut target = hit_node;
        let mut input = None;
        let mut grid = None;
        let mut checkbox = None;
        loop {
            let node = doc.get_node(target)?;
            if let Some(el) = node.element_data() {
                if &*el.name.local == "button" {
                    return None;
                }
                if let Some(id) = el.id.as_deref() {
                    if let Some(open) = id
                        .strip_prefix("checkbox-")
                        .and_then(|v| v.parse::<usize>().ok())
                    {
                        checkbox = Some(open);
                    }
                    if id.starts_with("table-menu-")
                        || id == "empty-line-tools"
                        || id == "slash-menu"
                        || id == "block-menu"
                        || id.starts_with("col-menu-")
                        || id.starts_with("row-menu-")
                        || id.starts_with("block-handle-")
                    {
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
        // Choose a row by y first. The right rail belongs to the edge cells,
        // while clicking below the table's bottom border targets the line underneath.
        let field = if let Some(field) = input {
            field
        } else if let Some(t) = grid {
            let below_table = doc
                .get_element_by_id(&format!("grid-scroll-{t}"))
                .and_then(|n| doc.get_client_bounding_rect(n))
                .is_some_and(|r| y > r.y + r.height);
            if below_table {
                let stops = self.stops();
                let last_row = self.tables.read()[t].rows.len().saturating_sub(1);
                let after = stops
                    .iter()
                    .position(|f| *f == Field::Cell(t, last_row, 0))
                    .and_then(|i| stops.get(i + 1))
                    .copied();
                match after {
                    Some(Field::Cell(nt, nr, _)) => self.column_at(&doc, nt, nr, x)?,
                    Some(next) => next,
                    None => self.column_at(&doc, t, last_row, x)?,
                }
            } else {
                let table = self.tables.read();
                let row = (0..table[t].rows.len())
                    .filter_map(|r| {
                        let b = rect(Field::Cell(t, r, 0))?;
                        Some((r, axis_distance(y, b.y, b.height)))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))?
                    .0;
                self.column_at(&doc, t, row, x)?
            }
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
        let proj = self.projection_peek(field);
        let pos = proj.display_to_source(offset);
        Some(Hit {
            field,
            pos,
            input: input == Some(field),
            checkbox,
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
                let target_menu = if r == 0 {
                    super::writing::TableMenu::Column(t, c)
                } else {
                    super::writing::TableMenu::Row(t, r)
                };
                self.table_menu.set(Some(target_menu));
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
        if let Some(open) = hit.checkbox {
            self.pressed_box.set(Some(open));
        } else if matches!(hit.field, Field::Body(_))
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
        if hit.checkbox.is_some() && !e.modifiers().shift() {
            self.drag.set(Some(Drag {
                field: hit.field,
                anchor,
                last: [anchor, hit.pos],
                across: false,
                native: false,
            }));
            e.prevent_default();
            return;
        }
        let will_expand_heading = previous != Some(hit.field)
            && *self.edit_mode.peek() == super::projection::EditMode::Markdown
            && self.projection_peek(hit.field).has_hidden_heading_prefix;
        if hit.input && !e.modifiers().shift() && !will_expand_heading {
            self.drag.set(Some(Drag {
                field: hit.field,
                anchor,
                last: [anchor, hit.pos],
                across: false,
                native: true,
            }));
            if self.span.peek().is_some() {
                self.span.set(None);
            }
            return; // Parley handles double clicks, word drags and platform caret affinity.
        }
        e.prevent_default();
        self.open(hit.field, [anchor, hit.pos]);
        self.drag.set(Some(Drag {
            field: hit.field,
            anchor,
            last: [anchor, hit.pos],
            across: false,
            native: false,
        }));
    }

    pub(super) fn pointer_move(mut self, e: PointerEvent) {
        let active_bd = *self.block_drag.peek();
        if let Some(mut bd) = active_bd {
            if !e
                .held_buttons()
                .contains(dioxus_html::input_data::MouseButton::Primary)
            {
                self.finish_block_drag();
                return;
            }
            let y = e.client_coordinates().y;
            if !bd.dragging && (y - bd.start_y).abs() < 5.0 {
                return;
            }
            if !bd.dragging {
                self.block_menu.set(None);
                bd.dragging = true;
            }
            if let Some((target, indicator_y)) = self.block_drop_target(y) {
                bd.target = target;
                bd.indicator_y = (target != bd.from && target != bd.from + 1).then_some(indicator_y);
            }
            if active_bd != Some(bd) {
                self.block_drag.set(Some(bd));
            }
            return;
        }
        let Some(mut drag) = *self.drag.peek() else {
            if self.boxes.peek().is_empty() {
                if self.hovered_box.peek().is_some() {
                    self.hovered_box.set(None);
                }
                return;
            }
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
        // Fast path: while natively dragging inside the original field's box, let Parley
        // handle selection natively without hit-testing or layout resolution.
        if drag.native
            && !drag.across
            && let Some(handle) = self.root.peek().clone()
        {
            let doc = handle.doc();
            let target_id = match drag.field {
                Field::Cell(t, r, c) => format!("slot-{t}-{r}-{c}"),
                other => other.id(),
            };
            if let Some(rect) = doc
                .get_element_by_id(&target_id)
                .and_then(|id| doc.get_client_bounding_rect(id))
                && p.x >= rect.x
                && p.x <= rect.x + rect.width
                && p.y >= rect.y
                && p.y <= rect.y + rect.height
            {
                return;
            }
        }
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
        if hit.field == drag.field && drag.native && !drag.across && hit.input {
            return;
        }
        e.prevent_default();
        let target_sel = [drag.anchor, hit.pos];
        let crossed = hit.field != drag.field;
        if *self.active.peek() == Some(hit.field) && drag.last == target_sel {
            return;
        }
        if crossed && !drag.across {
            drag.across = true;
        }
        drag.last = target_sel;
        self.drag.set(Some(drag));
        self.clear_cells();
        self.open(hit.field, target_sel);
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

    fn block_drop_target(self, client_y: f64) -> Option<(usize, f64)> {
        let handle = self.root.peek().clone()?;
        let doc = handle.doc();
        let paper = doc.get_client_bounding_rect(doc.get_element_by_id("paper")?)?;
        let md = self.contents.read();
        let total = super::blocks::split_blocks(&md).len();
        drop(md);
        if total == 0 {
            return None;
        }
        let mut tops: Vec<Option<f64>> = Vec::with_capacity(total);
        for i in 0..total {
            let y = doc
                .get_element_by_id(&format!("block-handle-{i}"))
                .and_then(|id| doc.get_client_bounding_rect(id))
                .map(|r| r.y);
            tops.push(y);
        }
        let mut present: Vec<(usize, f64)> = tops
            .iter()
            .enumerate()
            .filter_map(|(i, y)| y.map(|v| (i, v)))
            .collect();
        if present.is_empty() {
            return None;
        }
        present.sort_by(|a, b| a.1.total_cmp(&b.1));
        let bottom_edge = doc
            .get_element_by_id("body-surface")
            .and_then(|id| doc.get_client_bounding_rect(id))
            .map(|r| r.y + r.height)
            .unwrap_or(paper.y + paper.height - 64.0)
            .max(present.last().unwrap().1 + 28.0);
        let mut slot = 0;
        for (pos, &(idx, top)) in present.iter().enumerate() {
            let next_top = present
                .get(pos + 1)
                .map_or(bottom_edge, |&(_, ny)| ny);
            let mid = (top + next_top) * 0.5;
            if client_y > mid {
                slot = idx + 1;
            }
        }
        let indicator_y = if slot == 0 {
            (present[0].1 - paper.y - 6.0).max(0.0)
        } else if slot >= total {
            (bottom_edge - paper.y + 4.0).max(0.0)
        } else {
            let before = present
                .iter()
                .rev()
                .find(|&&(i, _)| i < slot)
                .map(|&(_, y)| y);
            let after = present.iter().find(|&&(i, _)| i >= slot).map(|&(_, y)| y);
            match (before, after) {
                (_, Some(a)) => (a - paper.y - 6.0).max(0.0),
                (Some(b), None) => (b + 28.0 - paper.y).max(0.0),
                (None, None) => 0.0,
            }
        };
        Some((slot, indicator_y))
    }

    pub(super) fn finish_block_drag(mut self) {
        let Some(bd) = self.block_drag.take() else {
            return;
        };
        if bd.dragging {
            self.move_block_to_slot(bd.from, bd.target);
        } else {
            let next = (*self.block_menu.peek() != Some(bd.from)).then_some(bd.from);
            self.block_menu.set(next);
        }
    }

    pub(super) fn end_drag(mut self) {
        if self.block_drag.peek().is_some() {
            self.finish_block_drag();
        }
        if self.drag.peek().is_some() {
            self.drag.set(None);
            if let Some(field) = *self.active.peek() {
                self.track_field(field);
            }
        }
    }
}
