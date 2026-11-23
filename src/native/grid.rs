use super::selection_model::SelectionInk;
use super::table::{self, Table};
use super::{EditorButton, Field, NodeHandle, Writing, mounted_field, report, selection};
use dioxus_native::prelude::*;
use pulldown_cmark::Alignment;

impl Writing {
    pub fn finish_table_line(mut self, enter: bool) -> bool {
        let Some(at) = *self.draft_table.peek() else {
            return false;
        };
        let md = self.contents.read().clone();
        let Some(grid) = table::tables(&md).into_iter().find(|g| g.range.start == at) else {
            return false;
        };
        self.draft_table.set(None);
        let t = self.tables.read().partition_point(|g| g.range.start < at);
        if enter && grid.rows.len() == 1 {
            self.add_row(t, 0);
        } else {
            self.focus_cell(t, usize::from(enter), 0);
        }
        true
    }

    pub fn insert_table(mut self, source_mode: bool) {
        let md = self.contents.read().clone();
        let selected = if source_mode {
            selection(self.root, Field::Source)
        } else {
            self.absolute_selection()
        };
        let mut pos = selected
            .or(*self.parked_selection.peek())
            .map_or(md.len(), |s| s[0].max(s[1]));
        if let Some(t) = self.tables.read().iter().find(|t| t.range.contains(&pos)) {
            pos = t.range.end;
        }
        let start = md[..pos].rfind('\n').map_or(0, |i| i + 1);
        let end = pos + md[pos..].find('\n').unwrap_or(md.len() - pos);
        let at = if md[..end].ends_with('\r') {
            end - 1
        } else {
            end
        };
        let eol = table::eol(&md);
        let before = if start < at {
            eol.repeat(2)
        } else {
            String::new()
        };
        let after = if md[at..].starts_with(&eol.repeat(2)) {
            ""
        } else if md[at..].starts_with(eol) {
            eol
        } else {
            ""
        };
        let grid = format!("| Column A | Column B |{eol}| --- | --- |{eol}|  |  |");
        let anchor = at + before.len() + 2;
        let edit = self
            .note
            .write()
            .replace(at..at, &format!("{before}{grid}{after}"));
        report(edit, self.message);
        let range = [anchor, anchor + "Column A".len()];
        if source_mode {
            self.focus.set(Some((Field::Source, range)));
        } else {
            self.open(self.field_at(anchor), range);
        }
    }

    fn focus_cell(mut self, t: usize, r: usize, c: usize) {
        self.contents.read();
        let bounds = self.tables.read()[t].cell(r, c);
        let field = Field::Cell(t, r, c);
        self.open(field, [bounds.start, bounds.end]);
        self.quiet_cell.set(Some(field));
        self.refresh_format_position();
    }

    fn add_row(mut self, t: usize, col: usize) {
        self.clear_cells();
        self.table_menu.set(None);
        self.table_menu_top.set(None);
        let md = self.contents.read().clone();
        let grid = self.tables.read()[t].clone();
        let edit = grid.append_row(&md);
        report(self.note.write().splice(&[edit]), self.message);
        self.focus_cell(t, grid.rows.len(), col);
    }

    fn add_column(mut self, t: usize) {
        self.clear_cells();
        self.table_menu.set(None);
        self.table_menu_top.set(None);
        let grid = self.tables.read()[t].clone();
        report(
            self.note.write().splice(&grid.append_column()),
            self.message,
        );
        self.focus_cell(t, 0, grid.width());
    }

    fn insert_row_at(mut self, t: usize, at: usize, col: usize) {
        self.clear_cells();
        self.table_menu.set(None);
        self.table_menu_top.set(None);
        let md = self.contents.read().clone();
        let grid = self.tables.read()[t].clone();
        if let Some(range) = self.absolute_selection() {
            self.note.peek().remember_selection(range);
        }
        let edit = grid.insert_row(&md, at);
        let result = self.note.write().splice(&[edit]);
        if result.is_ok() {
            self.focus_cell(t, at.clamp(1, grid.rows.len()), col);
        }
        report(result, self.message);
    }

    fn insert_column_at(mut self, t: usize, row: usize, col: usize) {
        self.clear_cells();
        self.table_menu.set(None);
        self.table_menu_top.set(None);
        let md = self.contents.read().clone();
        let grid = self.tables.read()[t].clone();
        if let Some(range) = self.absolute_selection() {
            self.note.peek().remember_selection(range);
        }
        let edits = grid.insert_column(&md, col);
        let result = self.note.write().splice(&edits);
        if result.is_ok() {
            let target_row = if col >= grid.width() { 0 } else { row };
            self.focus_cell(t, target_row, col.min(grid.width()));
        }
        report(result, self.message);
    }

    fn move_row_dir(mut self, t: usize, row: usize, col: usize, down: bool) {
        self.clear_cells();
        self.table_menu.set(None);
        self.table_menu_top.set(None);
        let md = self.contents.read().clone();
        let grid = self.tables.read()[t].clone();
        let edits = grid.move_row(&md, row, down);
        if edits.is_empty() {
            return;
        }
        if let Some(range) = self.absolute_selection() {
            self.note.peek().remember_selection(range);
        }
        let result = self.note.write().splice(&edits);
        if result.is_ok() {
            let target = if down { row + 1 } else { row - 1 };
            self.focus_cell(t, target, col);
        }
        report(result, self.message);
    }

    fn move_column_dir(mut self, t: usize, row: usize, col: usize, right: bool) {
        self.clear_cells();
        self.table_menu.set(None);
        self.table_menu_top.set(None);
        let md = self.contents.read().clone();
        let grid = self.tables.read()[t].clone();
        let edits = grid.move_column(&md, col, right);
        if edits.is_empty() {
            return;
        }
        if let Some(range) = self.absolute_selection() {
            self.note.peek().remember_selection(range);
        }
        let result = self.note.write().splice(&edits);
        if result.is_ok() {
            let target = if right { col + 1 } else { col - 1 };
            self.focus_cell(t, row, target);
        }
        report(result, self.message);
    }

    fn align_columns(mut self, t: usize, rect: table::CellRect, alignment: Alignment) {
        self.table_menu.set(None);
        self.table_menu_top.set(None);
        let md = self.contents.read().clone();
        let grid = self.tables.read()[t].clone();
        let edits = grid.set_alignment(&md, rect.cols(), alignment);
        if edits.is_empty() {
            return;
        }
        if let Some(range) = self.absolute_selection() {
            self.note.peek().remember_selection(range);
        }
        let result = self.note.write().splice(&edits);
        if result.is_ok() {
            self.focus_cell(t, rect.head[0], rect.head[1]);
        }
        report(result, self.message);
    }

    pub(super) fn dismiss_menu_at(mut self, x: f64, y: f64) {
        let Some(t) = *self.table_menu.peek() else {
            return;
        };
        let Some(root) = self.root.peek().clone() else {
            return;
        };
        let doc = root.doc();
        let inside = [format!("table-actions-{t}"), format!("table-menu-{t}")]
            .iter()
            .any(|id| {
                doc.get_element_by_id(id)
                    .and_then(|n| doc.get_client_bounding_rect(n))
                    .is_some_and(|b| {
                        x >= b.x && x < b.x + b.width && y >= b.y && y < b.y + b.height
                    })
            });
        if !inside {
            self.table_menu.set(None);
            self.table_menu_top.set(None);
        }
    }

    pub(super) fn position_table_menu(mut self) {
        let Some(index) = *self.table_menu.peek() else {
            return;
        };
        let Some(root) = self.root.peek().clone() else {
            return;
        };
        let doc = root.doc();
        let bounds = |id: &str| doc.get_client_bounding_rect(doc.get_element_by_id(id)?);
        let (Some(frame), Some(menu)) = (
            bounds(&format!("grid-{index}")),
            bounds(&format!("table-menu-{index}")),
        ) else {
            return;
        };
        let h = f64::from(doc.viewport().logical_size().1);
        let min_top = bounds("workspace-bar").map_or(6., |r| r.y + r.height + 6.);
        let max_bottom = h - 44.;
        let below = frame.y + frame.height;
        let above = frame.y - menu.height - 6.;
        let y = if below + menu.height <= max_bottom
            || (above < min_top && max_bottom - below >= frame.y - min_top)
        {
            below
        } else {
            above.max(min_top)
        };
        let top = Some((y - frame.y).round() as i32);
        if top != *self.table_menu_top.peek() {
            self.table_menu_top.set(top);
        }
    }

    pub(super) fn clear_cells(mut self) {
        if self.cells.peek().is_some() {
            self.cells.set(None);
        }
        if *self.cell_editing.peek() {
            self.cell_editing.set(false);
        }
    }

    pub(super) fn keep_cells_in(self, field: Field) {
        if let Some((index, rect)) = *self.cells.peek()
            && !matches!(field, Field::Cell(t,r,c) if t == index && rect.rows().contains(&r) && rect.cols().contains(&c))
        {
            self.clear_cells();
        }
    }

    pub(super) fn select_cells(mut self, t: usize, rect: table::CellRect) {
        let [r, c] = rect.head;
        self.table_menu.set(None);
        self.cells.set((rect.count() > 1).then_some((t, rect)));
        self.cell_editing.set(false);
        self.focus_cell(t, r, c);
    }

    fn clear_or_fill(mut self, t: usize, rect: table::CellRect, value: &str, r: usize, c: usize) {
        let edits = self.tables.read()[t].paste_rect(&self.contents.peek(), rect, value);
        if let Some(range) = self.absolute_selection() {
            self.note.peek().remember_selection(range);
        }
        let result = self.note.write().splice(&edits);
        if result.is_ok() {
            self.focus_cell(t, r, c);
        }
        self.cell_editing.set(false);
        report(result, self.message);
    }

    fn remove_axis(mut self, t: usize, rect: table::CellRect, columns: bool) {
        let grid = self.tables.read()[t].clone();
        let [r, c] = rect.head;
        let (edits, r, c) = if columns {
            let cols = rect.cols();
            (
                grid.delete_columns(&self.contents.peek(), cols.clone()),
                r,
                cols.start.min(grid.width().saturating_sub(cols.len() + 1)),
            )
        } else {
            let rows = rect.rows();
            (
                grid.delete_rows(rows.clone()).into_iter().collect(),
                rows.start
                    .min(grid.rows.len().saturating_sub(rows.len() + 1)),
                c,
            )
        };
        if edits.is_empty() {
            return;
        }
        if let Some(range) = self.absolute_selection() {
            self.note.peek().remember_selection(range);
        }
        self.clear_cells();
        self.table_menu.set(None);
        let result = self.note.write().splice(&edits);
        if result.is_ok() {
            self.focus_cell(t, r, c);
        }
        report(result, self.message);
    }

    fn range_key(mut self, t: usize, r: usize, c: usize, event: &KeyboardEvent) -> bool {
        let grid = self.tables.read();
        let height = grid[t].rows.len();
        let width = grid[t].width();
        drop(grid);
        let rect = self
            .cells
            .peek()
            .as_ref()
            .filter(|(index, rect)| {
                *index == t && rect.rows().end <= height && rect.cols().end <= width
            })
            .map(|(_, rect)| *rect);
        let mods = event.modifiers();
        let key = event.key();
        if mods.shift()
            && !mods.intersects(Modifiers::META | Modifiers::CONTROL)
            && (rect.is_some() || mods.alt())
            && let Key::Named(
                arrow @ (NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown),
            ) = key
        {
            let mut next = rect.unwrap_or(table::CellRect {
                anchor: [r, c],
                head: [r, c],
            });
            let [row, col] = next.head;
            next.head = match arrow {
                NamedKey::ArrowLeft => [row, col.saturating_sub(1)],
                NamedKey::ArrowRight => [row, (col + 1).min(width - 1)],
                NamedKey::ArrowUp => [row.saturating_sub(1), col],
                _ => [(row + 1).min(height - 1), col],
            };
            self.select_cells(t, next);
        } else if let Some(rect) = rect {
            let editing = *self.cell_editing.peek();
            match key {
                Key::Named(NamedKey::Tab) if (mods - Modifiers::SHIFT).is_empty() => {
                    let [row, col] = rect.step(r, c, mods.shift());
                    self.cell_editing.set(false);
                    self.focus_cell(t, row, col);
                }
                Key::Named(NamedKey::Escape) => {
                    self.clear_cells();
                    let end = self.tables.read()[t].cell(r, c).end;
                    self.open(Field::Cell(t, r, c), [end; 2]);
                }
                Key::Named(NamedKey::Delete | NamedKey::Backspace)
                    if mods.is_empty() && !editing =>
                {
                    self.clear_or_fill(t, rect, "", r, c)
                }
                Key::Character(ref k)
                    if !editing
                        && mods.intersects(Modifiers::META | Modifiers::CONTROL)
                        && !mods.shift()
                        && !mods.alt()
                        && matches!(k.to_ascii_lowercase().as_str(), "c" | "v" | "x") =>
                {
                    let Some(handle) = self.root.peek().clone() else {
                        return false;
                    };
                    let shell = handle.doc().shell_provider.clone();
                    if k.eq_ignore_ascii_case("v") {
                        if let Ok(text) = shell.get_clipboard_text() {
                            self.clear_or_fill(t, rect, &text, r, c);
                        }
                    } else {
                        let text = self.tables.read()[t].copy_rect(&self.contents.peek(), rect);
                        if shell.set_clipboard_text(text).is_ok() && k.eq_ignore_ascii_case("x") {
                            self.clear_or_fill(t, rect, "", r, c);
                        }
                    }
                }
                Key::Named(
                    NamedKey::ArrowLeft
                    | NamedKey::ArrowRight
                    | NamedKey::ArrowUp
                    | NamedKey::ArrowDown
                    | NamedKey::Enter,
                ) => {
                    self.clear_cells();
                    return false;
                }
                _ => return false,
            }
        } else {
            return false;
        }
        event.prevent_default();
        event.stop_propagation();
        true
    }

    fn cell_key(mut self, t: usize, r: usize, c: usize, event: KeyboardEvent) {
        if event.is_composing() {
            return;
        }
        let field = Field::Cell(t, r, c);
        if event.modifiers().shift()
            && matches!(
                event.key(),
                Key::Named(
                    NamedKey::ArrowLeft | NamedKey::ArrowRight | NamedKey::Home | NamedKey::End
                )
            )
        {
            self.quiet_cell.set(None);
        }
        if self.range_key(t, r, c, &event) || self.document_key(field, &event) {
            return;
        }
        if event
            .modifiers()
            .intersects(Modifiers::CONTROL | Modifiers::META)
            && matches!(event.key(), Key::Character(k) if k.eq_ignore_ascii_case("v"))
        {
            let clipboard = self
                .root
                .peek()
                .as_ref()
                .and_then(|h| h.doc().shell_provider.get_clipboard_text().ok());
            if let Some(text) = clipboard
                && text.contains(['\t', '\r', '\n'])
            {
                event.prevent_default();
                self.paste_cells(t, r, c, &text);
                return;
            }
        }
        let key = event.key();
        let grid = self.tables.read();
        let width = grid[t].width();
        let height = grid[t].rows.len();
        drop(grid);
        if event
            .modifiers()
            .intersects(Modifiers::CONTROL | Modifiers::META)
        {
            match key {
                Key::Named(NamedKey::Home) => self.focus_cell(t, 0, 0),
                Key::Named(NamedKey::End) => self.focus_cell(t, height - 1, width - 1),
                _ => return,
            }
            event.prevent_default();
            event.stop_propagation();
            return;
        }
        if event.modifiers() == Modifiers::ALT {
            match key {
                Key::Named(NamedKey::ArrowUp) => self.move_row_dir(t, r, c, false),
                Key::Named(NamedKey::ArrowDown) => self.move_row_dir(t, r, c, true),
                Key::Named(NamedKey::ArrowLeft) => self.move_column_dir(t, r, c, false),
                Key::Named(NamedKey::ArrowRight) => self.move_column_dir(t, r, c, true),
                _ => return,
            }
            event.prevent_default();
            event.stop_propagation();
            return;
        }
        if event.modifiers().contains(Modifiers::ALT) {
            return;
        }
        let shift = event.modifiers().contains(Modifiers::SHIFT);
        if matches!(key, Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown)) {
            self.vertical(field, key == Key::Named(NamedKey::ArrowDown), shift);
            event.prevent_default();
            event.stop_propagation();
            return;
        }
        self.clear_goal();
        let flat = r * width + c;
        let target = match key {
            Key::Named(NamedKey::Tab) => {
                if shift {
                    flat.checked_sub(1)
                } else {
                    Some(flat + 1)
                }
            }
            Key::Named(NamedKey::Enter) => {
                if shift {
                    r.checked_sub(1).map(|r| r * width + c)
                } else {
                    Some((r + 1) * width + c)
                }
            }
            Key::Named(NamedKey::Escape) => {
                event.prevent_default();
                event.stop_propagation();
                self.grid_exit(t, true);
                return;
            }
            _ => return,
        };
        event.prevent_default();
        event.stop_propagation();
        match target {
            Some(n) if n / width >= height => self.add_row(t, n % width),
            Some(n) => self.focus_cell(t, n / width, n % width),
            None => self.grid_exit(t, false),
        }
    }

    fn paste_cells(mut self, t: usize, r: usize, c: usize, value: &str) {
        let md = self.contents.read().clone();
        let edits = self.tables.read()[t].paste(&md, r, c, value);
        report(self.note.write().splice(&edits), self.message);
        self.focus_cell(t, r, c);
    }

    fn cell_input(mut self, t: usize, r: usize, c: usize, value: &str) {
        let field = Field::Cell(t, r, c);
        if self.replace_span_input(field, value) {
            return;
        }
        if self.cells.peek().is_some() {
            self.cell_editing.set(true);
        }
        let proj = self.projection(field);
        let raw_decoded = table::decoded(&self.contents.read()[self.tables.read()[t].cell(r, c)]);
        if proj.display != raw_decoded {
            if let Some((splices, next_caret)) = proj.map_input(value) {
                let result = self.note.write().splice(&splices);
                if result.is_ok() {
                    self.cursor_source.set(Some([next_caret; 2]));
                    let next_proj = self.projection(field);
                    if next_proj.display != value {
                        self.open(field, [next_caret; 2]);
                        return;
                    }
                }
                report(result, self.message);
                self.track_field(field);
                return;
            }
        }
        let edit = self.tables.read()[t].edit_cell(&self.contents.read(), r, c, value);
        report(self.note.write().splice(&[edit]), self.message);
        self.track_field(field);
    }
}

#[component]
pub fn Heading(writing: Writing, field: Field) -> Element {
    let md = writing.contents.read();
    let ranges = writing.heading(field, &md);
    let hashes = ranges.as_ref().map_or("", |(h, _)| &md[h.clone()]).to_owned();
    drop(md);
    let title = writing.projection(field).display;
    let level = hashes.len();
    let width = level * 6 + 12;
    let left = width + 5;
    let main = field == Field::Title;
    let label = if main {
        "Note title"
    } else {
        "Section heading"
    };
    let id = field.id();
    let prefix_id = if main {
        "heading-level".to_owned()
    } else {
        format!("{id}-level")
    };
    let height_slot = match field {
        Field::Title => 0,
        Field::Heading(i) => i + 1,
        _ => 0,
    };
    let min_h = if main {
        66
    } else {
        match level {
            1 => 62,
            2 => 44,
            3 => 37,
            4 => 33,
            5 => 29,
            _ => 26,
        }
    };
    let measured = writing
        .heading_heights
        .read()
        .get(height_slot)
        .copied()
        .unwrap_or(0);
    let height = measured.max(min_h);
    let style = format!("height: {height}px;");
    let tip = if cfg!(target_os = "macos") {
        "Heading level · click to cycle, # / Backspace, or ⌘1–6"
    } else {
        "Heading level · click to cycle, # / Backspace, or Ctrl+1–6"
    };
    rsx! {
        div { class: "title-row", class: if !main { "section-heading" },
            textarea { id, class: "note-title h{level}", class: if !main { "section-title" },
                aria_label: label, placeholder: if main { "Untitled" } else { "" }, value: "{title}", style,
                onmounted: move |e| {
                    if let Some(handle) = e.downcast::<NodeHandle>() { handle.doc_mut().resolve(0.0); }
                    mounted_field(e, field, writing.focus);
                    writing.refresh_boxes();
                },
                onfocus: move |_| writing.track_field(field), onclick: move |_| writing.pointer(field),
                onpointerup: move |_| writing.track_field(field),
                onkeydown: move |e| writing.key(field, e), onkeyup: move |_| writing.track_field(field),
                oninput: move |e| writing.input(field, &e.value()),
                "{title}"
            }
            if !hashes.is_empty() {
                span { id: prefix_id, class: "heading-level", aria_hidden: "true", title: tip,
                    style: "width: {width}px; left: -{left}px;",
                    onpointerdown: move |e| {
                        e.prevent_default();
                        e.stop_propagation();
                    },
                    onclick: move |e| {
                        e.stop_propagation();
                        let next = if e.modifiers().shift() {
                            level.saturating_sub(1)
                        } else if level >= 3 {
                            1
                        } else {
                            level + 1
                        };
                        writing.set_heading_level(field, next);
                    },
                    "{hashes}" }
            }
            {(writing.ink)().into_iter().filter(|ink| ink.field == field).map(|ink| ink.draw())}
            SelectionInk { writing, field }
        }
    }
}
#[component]
pub fn Prose(writing: Writing, index: usize, segmented: bool) -> Element {
    let field = Field::Body(index);
    let body = writing.projection(field).display;
    let min_h = if segmented { 26 } else { 235 };
    let height = writing
        .heights
        .read()
        .get(index)
        .copied()
        .unwrap_or(0)
        .max(body.split('\n').count() * 26)
        .max(min_h);
    let style = format!("height: {height}px; min-height: {min_h}px;");
    let hovered_open = *writing.hovered_box.read();
    let boxes: Vec<_> = (writing.boxes)()
        .into_iter()
        .filter(|b| b.field == field && !b.revealed)
        .collect();
    let over_box = hovered_open.is_some_and(|open| boxes.iter().any(|b| b.open == open));
    rsx! {
        div { class: "prose-block", class: if over_box { "over-checkbox" },
            textarea {
                id: field.id(), class: "body-editor", aria_label: "Edit note body", value: "{body}", style,
                aria_describedby: (writing.marker)().map(|_| "marker-hint"),
                onmounted: move |e| {
                    if let Some(handle) = e.downcast::<NodeHandle>() { handle.doc_mut().resolve(0.0); }
                    mounted_field(e, field, writing.focus);
                    writing.refresh_boxes();
                },
                onfocus: move |_| writing.track_field(field),
                onclick: move |e| writing.click_body(field, e),
                onpointerup: move |_| writing.track_field(field),
                onkeydown: move |e| writing.key(field, e),
                onkeyup: move |_| writing.track_field(field),
                onscroll: move |_| writing.refresh_boxes(),
                oninput: move |e| writing.input(field, &e.value()),
                "{body}"
            }
            {(writing.ink)().into_iter().filter(|ink| ink.field == field).map(|ink| ink.draw())}
            {boxes.into_iter().map(|b| b.draw(hovered_open == Some(b.open)))}
            SelectionInk { writing, field }
        }
    }
}

fn align(grid: &Table, c: usize) -> &'static str {
    match grid.align[c] {
        Alignment::Right => "right",
        Alignment::Center => "center",
        _ => "left",
    }
}

fn scope_label(rect: table::CellRect) -> String {
    fn label([row, col]: [usize; 2]) -> String {
        let mut n = col + 1;
        let mut letters = Vec::new();
        while n > 0 {
            n -= 1;
            letters.push((b'A' + (n % 26) as u8) as char);
            n /= 26;
        }
        format!(
            "{}{}",
            letters.into_iter().rev().collect::<String>(),
            row + 1
        )
    }
    if rect.count() == 1 {
        format!("Cell {}", label(rect.anchor))
    } else {
        format!(
            "{} – {}",
            label([rect.rows().start, rect.cols().start]),
            label([rect.rows().end - 1, rect.cols().end - 1])
        )
    }
}

#[component]
fn GridRow(
    writing: Writing,
    index: usize,
    row: usize,
    last: bool,
    values: Vec<(String, &'static str, usize)>,
) -> Element {
    rsx! {
        div { class: "grid-row", class: if last { "ledger-last" }, role: "row",
            for (col, (value, alignment, weight)) in values.into_iter().enumerate() {
                {
                    let field = Field::Cell(index, row, col);
                    let selected_rect = writing.cells.read().as_ref().filter(|(i,s)| *i==index && s.rows().contains(&row) && s.cols().contains(&col)).map(|(_,s)|*s);
                    let selected = selected_rect.is_some();
                    let active = *writing.active.read() == Some(field) && writing.span.read().is_none();
                    let whole_cell = selected && active && !*writing.cell_editing.read();
                    rsx! {
                        div { key: "{col}", id: "slot-{index}-{row}-{col}", class: "cell-slot ledger-cell align-{alignment}",
                            class: if row == 0 { "ledger-head" } else { "ledger-data" },
                            class: if col == 0 && row > 0 && alignment == "left" { "ledger-label" },
                            class: if row > 0 && alignment == "right" { "ledger-number" },
                            class: if selected { "range-cell" }, class: if active { "is-editing" },
                            class: if whole_cell { "range-target" },
                            aria_selected:selected.to_string(), flex_grow: weight.to_string(), role: if row == 0 { "columnheader" } else { "cell" },
                            div { class: "cell-field",
                                // A Blitz single-line input has an unconstrained Parley layout. Size its
                                // frame to the shaped text for alignment; let long values scroll normally.
                                if alignment != "left" || whole_cell {
                                    span { class: "cell-measure", class: if whole_cell { "range-value" }, aria_hidden: "true", "{value}" }
                                }
                                input {
                                    id: field.id(), "data-range-keys": if selected && !*writing.cell_editing.read() { "true" }, class: "grid-cell",
                                    aria_label: format!("Row {}, column {}", row + 1, col + 1), value,
                                    onmounted: move |e| mounted_field(e, field, writing.focus),
                                    onfocus: move |_| writing.track_field(field),
                                    onpointerup: move |_| writing.track_field(field),
                                    onkeydown: move |e| writing.cell_key(index, row, col, e),
                                    onkeyup: move |_| writing.track_field(field),
                                    oninput: move |e| writing.cell_input(index, row, col, &e.value()),
                                }
                                {(writing.ink)().into_iter().filter(|ink| ink.field == field).map(|ink| ink.draw())}
                                SelectionInk { writing, field }
                            }
                            if let Some(s) = selected_rect {
                                span { class:"range-outline", aria_hidden:"true",
                                    style:format!("border-width:{}px {}px {}px {}px;",2*u8::from(row==s.rows().start),2*u8::from(col+1==s.cols().end),2*u8::from(row+1==s.rows().end),2*u8::from(col==s.cols().start)) }
                            } else if active {
                                span { class: "cell-outline", aria_hidden: "true" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn Grid(writing: Writing, index: usize) -> Element {
    let mut writing = writing;
    let mut hovered = use_signal(|| false);
    let mut within = use_signal(|| false);
    // Fit columns when the grid opens or its column count changes. Ordinary
    // typing must never move the cell underneath a stationary pointer.
    let width = use_memo(move || writing.tables.read().get(index).map_or(0, Table::width));
    let weights = use_memo(move || {
        let width = width();
        let tables = writing.tables.peek();
        let md = writing.contents.peek();
        let Some(grid) = tables.get(index) else {
            return Vec::new();
        };
        (0..width)
            .map(|c| {
                let longest = (0..grid.rows.len())
                    .map(|r| md[grid.cell(r, c)].chars().take(42).count())
                    .max()
                    .unwrap_or(0);
                longest.max(7) + 5
            })
            .collect::<Vec<_>>()
    });
    // Programmatic caret restoration updates Blitz focus directly, including
    // when a key crosses into a grid. Normal focusin/out cover pointer and Tab.
    use_effect(move || {
        writing.focus.read();
        let root = writing.root.read();
        let Some(handle) = root.as_ref() else { return };
        let doc = handle.doc();
        let Some(grid) = doc.get_element_by_id(&format!("grid-{index}")) else {
            return;
        };
        let mut target = doc.get_focussed_node_id();
        while let Some(id) = target {
            if id == grid {
                break;
            }
            target = doc.get_node(id).and_then(|node| node.parent);
        }
        within.set(target.is_some());
    });
    let tables = writing.tables.read();
    let Some(grid) = tables.get(index) else {
        return rsx! {};
    };
    let selected = match *writing.active.read() {
        Some(Field::Cell(t, r, c))
            if t == index
                && r < grid.rows.len()
                && c < grid.width()
                && writing.span.read().is_none() =>
        {
            Some((r, c))
        }
        _ => None,
    };
    let rect = selected.map(|(r, c)| {
        writing
            .cells
            .read()
            .as_ref()
            .filter(|(i, _)| *i == index)
            .map(|(_, rect)| *rect)
            .unwrap_or(table::CellRect {
                anchor: [r, c],
                head: [r, c],
            })
    });
    let menu = *writing.table_menu.read() == Some(index);
    let col_label = if rect.is_some_and(|rect| rect.cols().len() > 1) {
        "Delete selected columns"
    } else {
        "Delete column"
    };
    let row_label = if rect.is_some_and(|rect| rect.rows().len() > 1) {
        "Delete selected rows"
    } else {
        "Delete row"
    };
    rsx! {
        div { id: "grid-{index}", class: "grid-block ledger", class: if rect.is_some() || hovered() || within() { "engaged" },
            onkeydown:move |e| {
                if menu && e.key()==Key::Named(NamedKey::Escape) {
                    e.prevent_default(); e.stop_propagation(); writing.table_menu.set(None); writing.table_menu_top.set(None);
                    if let Some((r,c))=selected && let Some(root)=writing.root.peek().clone() {
                        let mut doc = root.doc_mut();
                        if let Some(id) = doc.get_element_by_id(&Field::Cell(index,r,c).id()) { doc.set_focus_to(id); }
                    }
                } else if selected.is_some() && e.key() == Key::Named(NamedKey::F10) && e.modifiers() == Modifiers::SHIFT {
                    e.prevent_default(); e.stop_propagation(); writing.table_menu_top.set(None); writing.table_menu.set(Some(index));
                }
            },
            onpointerenter: move |_| hovered.set(true), onpointerleave: move |_| hovered.set(false),
            onfocusin: move |_| within.set(true), onfocusout: move |_| within.set(false),
            div { id: "grid-scroll-{index}", class: "grid-scroll", role: "table", aria_label: "Markdown table",
                for (r, _) in grid.rows.iter().enumerate() {
                    GridRow { key: "{r}", writing, index, row: r, last: r + 1 == grid.rows.len(),
                        values: weights.read().iter().enumerate().map(|(c, &weight)| (writing.projection(Field::Cell(index, r, c)).display, align(grid, c), weight)).collect()
                    }
                }
            }
            div { class: "grid-edge col",
                EditorButton { id: format!("add-column-{index}"), class: "ledger-add-col", label: "Add column", tip: "Add column", icon: super::ToolIcon::Plus, onactivate: move |_| writing.add_column(index) }
            }
            div { class: "table-bar ledger-tools",
                if let Some(rect) = rect {
                    {
                        let multi = rect.count() > 1;
                        let [r, c] = rect.head;
                        let col_align = grid.align.get(c).copied().unwrap_or(Alignment::None);
                        rsx! {
                            div { id: "table-bar-{index}", class: "ledger-bar-group",
                                onpointerdown: move |e| e.prevent_default(),
                                span { class: "ledger-bar-scope", "{scope_label(rect)}" }
                                if multi {
                                    span { class: "cell-count", "{rect.count()} cells" }
                                    EditorButton { id: format!("clear-cells-{index}"), class: "ledger-chip", label: "Clear selected cells", glyph: "Clear",
                                        onactivate: move |_| { writing.clear_or_fill(index, rect, "", r, c); } }
                                    EditorButton { id: format!("delete-row-{index}"), class: "ledger-chip remove", label: row_label, glyph: "Del rows",
                                        disabled: rect.rows().start == 0, onactivate: move |_| writing.remove_axis(index, rect, false) }
                                    EditorButton { id: format!("delete-column-{index}"), class: "ledger-chip remove", label: col_label, glyph: "Del cols",
                                        disabled: rect.cols().len() >= grid.width(), onactivate: move |_| writing.remove_axis(index, rect, true) }
                                } else {
                                    span { class: "ledger-bar-tag", "Row" }
                                    EditorButton { id: format!("move-row-up-{index}"), class: "ledger-mini", label: "Move row up", tip: "Move row up (⌥↑)", glyph: "↑",
                                        disabled: r <= 1, onactivate: move |_| writing.move_row_dir(index, r, c, false) }
                                    EditorButton { id: format!("move-row-down-{index}"), class: "ledger-mini", label: "Move row down", tip: "Move row down (⌥↓)", glyph: "↓",
                                        disabled: r == 0 || r + 1 >= grid.rows.len(), onactivate: move |_| writing.move_row_dir(index, r, c, true) }
                                    EditorButton { id: format!("insert-row-above-{index}"), class: "ledger-mini", label: "Insert row above", tip: "Insert row above", glyph: "+↑",
                                        disabled: r == 0, onactivate: move |_| writing.insert_row_at(index, r, c) }
                                    EditorButton { id: format!("delete-row-{index}"), class: "ledger-mini remove", label: row_label, tip: "Delete row", glyph: "×",
                                        disabled: rect.rows().start == 0, onactivate: move |_| writing.remove_axis(index, rect, false) }
                                    span { class: "ledger-bar-sep", aria_hidden: "true" }
                                    span { class: "ledger-bar-tag", "Col" }
                                    EditorButton { id: format!("move-col-left-{index}"), class: "ledger-mini", label: "Move column left", tip: "Move column left (⌥←)", glyph: "←",
                                        disabled: c == 0, onactivate: move |_| writing.move_column_dir(index, r, c, false) }
                                    EditorButton { id: format!("move-col-right-{index}"), class: "ledger-mini", label: "Move column right", tip: "Move column right (⌥→)", glyph: "→",
                                        disabled: c + 1 >= grid.width(), onactivate: move |_| writing.move_column_dir(index, r, c, true) }
                                    EditorButton { id: format!("insert-col-left-{index}"), class: "ledger-mini", label: "Insert column left", tip: "Insert column left", glyph: "+←",
                                        onactivate: move |_| writing.insert_column_at(index, r, c) }
                                    EditorButton { id: format!("delete-column-{index}"), class: "ledger-mini remove", label: col_label, tip: "Delete column", glyph: "×",
                                        disabled: rect.cols().len() >= grid.width(), onactivate: move |_| writing.remove_axis(index, rect, true) }
                                    span { class: "ledger-bar-sep", aria_hidden: "true" }
                                    EditorButton { id: format!("align-left-{index}"), class: "ledger-mini", label: "Align left", glyph: "L", selected: matches!(col_align, Alignment::Left | Alignment::None),
                                        onactivate: move |_| writing.align_columns(index, rect, Alignment::Left) }
                                    EditorButton { id: format!("align-center-{index}"), class: "ledger-mini", label: "Align center", glyph: "C", selected: col_align == Alignment::Center,
                                        onactivate: move |_| writing.align_columns(index, rect, Alignment::Center) }
                                    EditorButton { id: format!("align-right-{index}"), class: "ledger-mini", label: "Align right", glyph: "R", selected: col_align == Alignment::Right,
                                        onactivate: move |_| writing.align_columns(index, rect, Alignment::Right) }
                                }
                            }
                        }
                    }
                }
                EditorButton { id: format!("add-row-{index}"), class: "ledger-add-row", label: "Add row", tip: "Add row", icon: super::ToolIcon::Plus, glyph: "Add row", onactivate: move |_| writing.add_row(index, 0) }
            }
        }
    }
}
