use super::selection_model::SelectionInk;
use crate::platform::text;
use crate::table::{self, Table};
use super::{Field, Writing, mounted_field, report, selection};
use crate::ui::{Button, Menu, MenuItem, MenuRow, MenuSeparator, Toolbar, ToolbarLook, ToolbarSep, Variant};
use crate::prelude::*;
use pulldown_cmark::Alignment;

/// The tables' interface state.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct TableUi {
    /// The selected cells: a table and a rectangle of its cells.
    pub cells: Signal<Option<(usize, table::CellRect)>>,
    /// A cell's text is being edited (rather than its cells selected).
    pub editing: Signal<bool>,
    /// The open column or row menu.
    pub menu: Signal<Option<super::writing::TableMenu>>,
    /// A cell a command opened: its format bar stays hidden until you type.
    pub quiet: Signal<Option<Field>>,
    /// Laid-out height of each table row, header first, so the row strip
    /// beside a table follows rows that grow with wrapped cell text.
    pub row_heights: Signal<Vec<Vec<f64>>>,
}

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
        self.grid.quiet.set(Some(field));
        self.refresh_format_position();
    }

    fn add_row(mut self, t: usize, col: usize) {
        self.clear_cells();
        self.grid.menu.set(None);
        let md = self.contents.read().clone();
        let grid = self.tables.read()[t].clone();
        let edit = grid.append_row(&md);
        report(self.note.write().splice(&[edit]), self.message);
        self.focus_cell(t, grid.rows.len(), col);
    }

    fn insert_row_at(mut self, t: usize, at: usize, col: usize) {
        self.clear_cells();
        self.grid.menu.set(None);
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
        self.grid.menu.set(None);
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
        self.grid.menu.set(None);
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
        self.grid.menu.set(None);
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
        self.grid.menu.set(None);
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
        let Some(root) = self.root.peek().clone() else {
            return;
        };
        let doc = root.doc();
        let inside = |id: &str| {
            doc.get_element_by_id(id)
                .and_then(|n| doc.get_client_bounding_rect(n))
                .is_some_and(|b| x >= b.x && x < b.x + b.width && y >= b.y && y < b.y + b.height)
        };
        let t_menu = *self.grid.menu.peek();
        if let Some(menu) = t_menu {
            let (btn_id, pop_id) = match menu {
                super::writing::TableMenu::Column(t, c) => {
                    (format!("col-menu-btn-{t}-{c}"), format!("col-menu-{t}-{c}"))
                }
                super::writing::TableMenu::Row(t, r) => {
                    (format!("row-menu-btn-{t}-{r}"), format!("row-menu-{t}-{r}"))
                }
            };
            if !inside(&btn_id) && !inside(&pop_id) {
                self.grid.menu.set(None);
            }
        }
        let b_menu = *self.handles.menu.peek();
        if let Some(b) = b_menu
            && !inside("block-menu")
            && !inside(&format!("block-handle-{b}"))
        {
            self.handles.menu.set(None);
        }
        if self.slash.menu.peek().is_some() && !inside("slash-menu") {
            self.slash.menu.set(None);
        }
        let c_menu = *self.callout_menu.peek();
        if let Some(start) = c_menu
            && !inside("callout-menu")
            && !inside(&format!("callout-badge-{start}"))
        {
            self.callout_menu.set(None);
        }
        let lang_menu = *self.code_lang_menu.peek();
        if let Some(start) = lang_menu
            && !inside("code-lang-menu")
            && !inside(&format!("code-lang-{start}"))
        {
            self.code_lang_menu.set(None);
        }
        if self.links.card.peek().is_some() && !inside("link-card") {
            if *self.links.editing.peek() || !inside("paper") {
                self.dismiss_link_card();
            }
        }
    }

    pub(super) fn clear_cells(mut self) {
        if self.grid.cells.peek().is_some() {
            self.grid.cells.set(None);
        }
        if *self.grid.editing.peek() {
            self.grid.editing.set(false);
        }
    }

    pub(super) fn keep_cells_in(self, field: Field) {
        if let Some((index, rect)) = *self.grid.cells.peek()
            && !matches!(field, Field::Cell(t,r,c) if t == index && rect.rows().contains(&r) && rect.cols().contains(&c))
        {
            self.clear_cells();
        }
    }

    pub(super) fn select_cells(mut self, t: usize, rect: table::CellRect) {
        let [r, c] = rect.head;
        self.grid.menu.set(None);
        self.grid.cells.set((rect.count() > 1).then_some((t, rect)));
        self.grid.editing.set(false);
        self.focus_cell(t, r, c);
    }

    pub(super) fn clear_or_fill(mut self, t: usize, rect: table::CellRect, value: &str, r: usize, c: usize) {
        let md = self.contents.read().clone();
        let edits = self.tables.read()[t].paste_rect(&md, rect, value);
        if let Some(range) = self.absolute_selection() {
            self.note.peek().remember_selection(range);
        }
        let result = self.note.write().splice(&edits);
        if result.is_ok() {
            self.focus_cell(t, r, c);
        }
        self.grid.editing.set(false);
        report(result, self.message);
    }

    fn remove_axis(mut self, t: usize, rect: table::CellRect, columns: bool) {
        let md = self.contents.read().clone();
        let grid = self.tables.read()[t].clone();
        let [r, c] = rect.head;
        let (edits, r, c) = if columns {
            let cols = rect.cols();
            (
                grid.delete_columns(&md, cols.clone()),
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
        self.grid.menu.set(None);
        let result = self.note.write().splice(&edits);
        if result.is_ok() {
            self.focus_cell(t, r, c);
        }
        report(result, self.message);
    }

    /// The selected range of cells in table `t`, while it still fits the table.
    pub(super) fn cell_range(self, t: usize) -> Option<table::CellRect> {
        let grid = self.tables.read();
        let (height, width) = (grid[t].rows.len(), grid[t].width());
        self.grid.cells
            .peek()
            .as_ref()
            .filter(|(index, rect)| {
                *index == t && rect.rows().end <= height && rect.cols().end <= width
            })
            .map(|(_, rect)| *rect)
    }

    fn range_key(mut self, t: usize, r: usize, c: usize, event: &KeyboardEvent) -> bool {
        let grid = self.tables.read();
        let height = grid[t].rows.len();
        let width = grid[t].width();
        drop(grid);
        let rect = self.cell_range(t);
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
            let editing = *self.grid.editing.peek();
            match key {
                Key::Named(NamedKey::Tab) if (mods - Modifiers::SHIFT).is_empty() => {
                    let [row, col] = rect.step(r, c, mods.shift());
                    self.grid.editing.set(false);
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
                        && matches!(k.to_ascii_lowercase().as_str(), "c" | "x") =>
                {
                    let Some(handle) = self.root.peek().clone() else {
                        return false;
                    };
                    let shell = handle.doc().shell_provider.clone();
                    let md = self.contents.read().clone();
                    let text = self.tables.read()[t].copy_rect(&md, rect);
                    if shell.set_clipboard_text(text).is_ok() && k.eq_ignore_ascii_case("x") {
                        self.clear_or_fill(t, rect, "", r, c);
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
            self.grid.quiet.set(None);
        }
        if self.range_key(t, r, c, &event) || self.document_key(field, &event) {
            return;
        }
        let key = event.key();
        // A cell is one line: no chord of Enter may put a line break in it.
        if key == Key::Named(NamedKey::Enter)
            && event.modifiers().intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT)
        {
            event.prevent_default();
            return;
        }
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
                if self.links.card.peek().is_some() {
                    self.dismiss_link_card();
                } else {
                    self.grid_exit(t, true);
                }
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

    pub(super) fn paste_cells(mut self, t: usize, r: usize, c: usize, value: &str) {
        let md = self.contents.read().clone();
        let edits = self.tables.read()[t].paste(&md, r, c, value);
        report(self.note.write().splice(&edits), self.message);
        self.focus_cell(t, r, c);
    }

    fn cell_input(mut self, t: usize, r: usize, c: usize, value: &str) {
        // A Markdown cell is one line. Enter moves between rows, so a line break can
        // only arrive inside inserted text.
        let value = value.replace(['\r', '\n'], " ");
        let value = value.as_str();
        let field = Field::Cell(t, r, c);
        if self.replace_span_input(field, value) {
            return;
        }
        if self.grid.cells.peek().is_some() {
            self.grid.editing.set(true);
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
    let main = field == Field::Title;
    let label = if main {
        "Note title"
    } else {
        "Section heading"
    };
    let id = field.id();
    let height_slot = match field {
        Field::Title => 0,
        Field::Heading(i) => i + 1,
        _ => 0,
    };
    let min_h = if main {
        44
    } else {
        match level {
            1 => 42,
            2 => 34,
            3 => 28,
            4 => 25,
            5 => 23,
            _ => 21,
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
    let is_focused = *writing.active.read() == Some(field);
    let block_pos = writing.range(field, &writing.contents.read()).start;
    let logical_blocks = writing.logical_blocks.read();
    let block_idx = super::blocks::block_index_at(&logical_blocks, block_pos);
    let total_blocks = logical_blocks.len();
    drop(logical_blocks);
    let dragging_this = block_idx.is_some_and(|b| {
        writing
            .handles.drag
            .read()
            .is_some_and(|d| d.dragging && d.from == b)
    });
    rsx! {
        div { class: "title-row block-wrap", class: if !main { "section-heading" }, class: if is_focused { "is-active-block" }, class: if dragging_this { "is-dragging-source" },
            if let Some(bidx) = block_idx {
                BlockHandle { writing, block_idx: bidx, total: total_blocks, top: if main { 10 } else { 5 } }
            }
            textarea { id, class: "note-title h{level}", class: if !main { "section-title" },
                aria_label: label, placeholder: if main { "Untitled" } else { "" }, value: "{title}", style,
                onmounted: move |e| {
                    if let Some(handle) = crate::platform::mounted(&e) { handle.doc_mut().resolve(0.0); }
                    mounted_field(e, field, writing.focus);
                    writing.refresh_boxes();
                    writing.refresh_link_card();
                },
                onfocus: move |_| writing.track_field(field), onclick: move |_| writing.pointer(field),
                onpointerup: move |_| writing.track_field(field),
                onkeydown: move |e| writing.key(field, e), onkeyup: move |_| writing.track_field(field),
                oninput: move |e| writing.input(field, &e.value()),
                "{title}"
            }
            {(writing.ink)().into_iter().filter(|ink| ink.field == field).map(|ink| ink.draw())}
            crate::comments::CommentInk { writing, field }
            SelectionInk { writing, field }
            super::markdown::CaretOverlay { writing, field }
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
    let boxes: Vec<_> = (writing.boxes)()
        .into_iter()
        .filter(|b| b.field == field && !b.revealed)
        .collect();
    let is_focused = *writing.active.read() == Some(field);
    let md_ref = writing.contents.read();
    let field_range = writing.range(field, &md_ref);
    let logical_blocks = writing.logical_blocks.read();
    let total_blocks = logical_blocks.len();
    let proj = writing.projection(field);
    let root_handle = writing.root.read().clone();
    let prose_handles: Vec<(usize, i32)> = logical_blocks
        .iter()
        .enumerate()
        .filter(|(_, r)| r.end > field_range.start && r.start < field_range.end)
        .map(|(bidx, r)| {
            let start = r.start.max(field_range.start);
            let measured_top = root_handle.as_ref().and_then(|h| {
                let doc = h.doc();
                let node = doc
                    .get_element_by_id(&field.id())
                    .and_then(|id| doc.get_node(id))?;
                let input = node.element_data()?.text_input_data()?;
                let layout = input.editor.try_layout()?;
                let disp = proj.source_to_display(start);
                let cluster = text::Cluster::from_byte_index(layout, disp)?;
                let scale = layout.scale();
                let y = (cluster.line().metrics().block_min_coord / scale - input.scroll_offset)
                    .round() as i32;
                Some(y + 1)
            });
            let fallback_top = (md_ref[field_range.start..start]
                .bytes()
                .filter(|b| *b == b'\n')
                .count() as i32)
                * 25
                + 1;
            (bidx, measured_top.unwrap_or(fallback_top).max(0))
        })
        .collect();
    drop(logical_blocks);
    drop(md_ref);
    rsx! {
        div { class: "prose-wrap block-wrap", class: if is_focused { "is-active-block" },
            for (bidx, top) in prose_handles {
                BlockHandle { key: "bh-{bidx}", writing, block_idx: bidx, total: total_blocks, top }
            }
            {(writing.callout_boxes)().into_iter().filter(|cb| cb.field == field).map(|cb| cb.draw())}
            {(writing.code_boxes)().into_iter().filter(|cb| cb.field == field).map(|cb| cb.draw())}
            div { class: "prose-block",
                textarea {
                    id: field.id(), class: "body-editor", aria_label: "Edit note body", value: "{body}", style,
                    aria_describedby: (writing.marker)().map(|_| "marker-hint"),
                    onmounted: move |e| {
                        if let Some(handle) = crate::platform::mounted(&e) { handle.doc_mut().resolve(0.0); }
                        mounted_field(e, field, writing.focus);
                        writing.refresh_boxes();
                        writing.refresh_link_card();
                    },
                    onfocus: move |_| writing.track_field(field),
                    onclick: move |e| writing.click_body(field, e),
                    onpointerup: move |_| writing.track_field(field),
                    onkeydown: move |e| writing.key(field, e),
                    onkeyup: move |_| writing.track_field(field),
                    onscroll: move |_| {
                        writing.refresh_boxes();
                        writing.refresh_link_card();
                    },
                    oninput: move |e| writing.input(field, &e.value()),
                    "{body}"
                }
                {(writing.ink)().into_iter().filter(|ink| ink.field == field).map(|ink| ink.draw())}
                {boxes.into_iter().map(|b| b.draw(writing))}
                crate::comments::CommentInk { writing, field }
                SelectionInk { writing, field }
                super::markdown::CaretOverlay { writing, field }
                EmptyLineBar { writing, field }
            }
            {(writing.callout_boxes)().into_iter().filter(|cb| cb.field == field).map(|cb| cb.draw_bar())}
            {(writing.callout_boxes)().into_iter().filter(|cb| cb.field == field && !cb.revealed).map(|cb| cb.draw_badge(writing))}
            {(writing.code_boxes)().into_iter().filter(|cb| cb.field == field).map(|cb| cb.draw_tools(writing))}
        }
    }
}

#[component]
pub fn BlockHandle(writing: Writing, block_idx: usize, total: usize, top: i32) -> Element {
    let open = *writing.handles.menu.read() == Some(block_idx);
    let is_dragging = writing
        .handles.drag
        .read()
        .is_some_and(|d| d.dragging && d.from == block_idx);
    rsx! {
        div {
            class: "block-handle-anchor",
            class: if open { "menu-open" },
            class: if is_dragging { "is-dragging" },
            style: "top:{top}px;",
            button {
                id: format!("block-handle-{block_idx}"),
                class: "block-handle",
                aria_label: "Drag to move or click for block actions",
                title: "Drag to move · Click for menu",
                aria_expanded: if open { "true" } else { "false" },
                onpointerdown: move |e| {
                    if e.trigger_button() == Some(dioxus_html::input_data::MouseButton::Primary) {
                        e.prevent_default();
                        e.stop_propagation();
                        writing.start_block_drag(block_idx, e.client_coordinates().y);
                    }
                },
                onkeydown: move |e| {
                    if matches!(e.key(), Key::Named(NamedKey::Enter))
                        || matches!(&e.key(), Key::Character(s) if s == " ")
                    {
                        e.prevent_default();
                        e.stop_propagation();
                        let next = (*writing.handles.menu.peek() != Some(block_idx)).then_some(block_idx);
                        writing.show_block_menu(next);
                    }
                },
                {crate::Icon::Grip.draw()}
            }
            if open {
                Menu { id: "block-menu", class: "absolute left-0 top-[26px] z-[12] w-[172px]", label: "Block menu",
                    MenuItem { id: "block-move-up", label: "Move up", badge: "↑", disabled: block_idx == 0,
                        onclick: move |_| writing.move_block_dir(block_idx, false) }
                    MenuItem { id: "block-move-down", label: "Move down", badge: "↓", disabled: block_idx + 1 >= total,
                        onclick: move |_| writing.move_block_dir(block_idx, true) }
                    MenuItem { id: "block-duplicate", label: "Duplicate", badge: "+",
                        onclick: move |_| writing.duplicate_block_at(block_idx) }
                    MenuSeparator {}
                    MenuItem { id: "block-delete", label: "Delete", badge: "×", danger: true,
                        onclick: move |_| writing.delete_block_at(block_idx) }
                }
            }
        }
    }
}

#[component]
pub fn EmptyLineBar(writing: Writing, field: Field) -> Element {
    let Some((target, x, y)) = (writing.empty_line)() else {
        return rsx! {};
    };
    if target != field {
        return rsx! {};
    }
    use crate::platform::shortcut;
    rsx! {
        Toolbar {
            id: "empty-line-tools",
            class: "absolute z-[4]",
            label: "Insert block",
            look: ToolbarLook::Inline,
            keep_focus: true,
            contain: true,
            style: "left:{x}px; top:{y}px;",
            Button { id: "empty-h1", variant: Variant::Icon, label: "Heading 1", text: "H1", tip: format!("Heading 1 ({})", shortcut("1")),
                onclick: move |_| { writing.set_heading_level(field, 1); } }
            Button { id: "empty-h2", variant: Variant::Icon, label: "Heading 2", text: "H2", tip: format!("Heading 2 ({})", shortcut("2")),
                onclick: move |_| { writing.set_heading_level(field, 2); } }
            Button { id: "empty-h3", variant: Variant::Icon, label: "Heading 3", text: "H3", tip: format!("Heading 3 ({})", shortcut("3")),
                onclick: move |_| { writing.set_heading_level(field, 3); } }
            ToolbarSep {}
            Button { id: "empty-bullet", variant: Variant::Icon, label: "Bullet list", icon: crate::Icon::List,
                onclick: move |_| { writing.toggle_line_prefix("- "); } }
            Button { id: "empty-task", variant: Variant::Icon, label: "Checklist", tip: format!("Checklist ({})", shortcut("Enter")), icon: crate::Icon::Task,
                onclick: move |_| { writing.toggle_line_task(field); } }
            Button { id: "empty-quote", variant: Variant::Icon, label: "Quote", tip: "Blockquote", icon: crate::Icon::Quote,
                onclick: move |_| { writing.toggle_line_prefix("> "); } }
            Button { id: "empty-callout", variant: Variant::Icon, label: "Callout", tip: "Callout box", icon: crate::Icon::Info,
                onclick: move |_| { writing.insert_callout(margin_editor_core::callout::CalloutKind::Note); } }
            Button { id: "empty-code", variant: Variant::Icon, label: "Code block", icon: crate::Icon::Code,
                onclick: move |_| { writing.insert_code_block(); } }
            Button { id: "empty-table", variant: Variant::Icon, label: "Table", tip: "Insert table", icon: crate::Icon::Table,
                onclick: move |_| { writing.insert_table(false); } }
            Button { id: "empty-image", variant: Variant::Icon, label: "Image", tip: "Insert image", icon: crate::Icon::Image,
                onclick: move |_| { writing.insert_image(); } }
            Button { id: "empty-rule", variant: Variant::Icon, label: "Divider", tip: "Horizontal rule", icon: crate::Icon::Rule,
                onclick: move |_| { writing.insert_rule(); } }
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
fn ColStrip(
    writing: Writing,
    index: usize,
    mut hovered: Signal<bool>,
    mut hovered_col: Signal<Option<usize>>,
    mut hovered_row: Signal<Option<usize>>,
    weights: Vec<usize>,
    alignments: Vec<Alignment>,
    width_count: usize,
    active_row: usize,
    multi_active: bool,
    open_col: Option<usize>,
    scroll_x: Signal<f64>,
) -> Element {
    let hover_c = (*hovered_col.read()).filter(|&c| c < width_count);
    let target_col = if multi_active {
        None
    } else {
        open_col.or(hover_c)
    };
    let Some(c) = target_col.filter(|&c| c < width_count) else {
        return rsx! {};
    };
    let col_open = open_col == Some(c);
    let col_align = alignments.get(c).copied().unwrap_or(Alignment::None);
    let col_rect = table::CellRect {
        anchor: [active_row, c],
        head: [active_row, c],
    };
    let weight_c = weights.get(c).copied().unwrap_or(12);
    let align_start = c == 0 && width_count > 1 && weight_c < 18;
    let align_end = c + 1 == width_count && width_count > 1 && weight_c < 18;
    const COL_HANDLE_BASE: &str = "ledger-col-handle flex items-center justify-center flex-none w-7 h-[15px] m-0 p-0 border border-solid border-[rgba(202,198,190,0.65)] rounded-full text-faint bg-[rgba(252,251,248,0.76)] shadow-[0_1px_2px_rgba(22,21,44,0.06)] cursor-pointer select-none pointer-events-auto [&_.tool-icon]:w-[13px] [&_.tool-icon]:h-[13px] [&_.tool-icon_svg]:w-[13px] [&_.tool-icon_svg]:h-[13px] hover:text-on-accent hover:bg-accent hover:border-accent hover:shadow-[0_2px_6px_rgba(22,21,44,0.18)] focus-visible:text-on-accent focus-visible:bg-accent focus-visible:border-accent focus-visible:shadow-[0_2px_6px_rgba(22,21,44,0.18)] focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1 [&.open]:text-on-accent [&.open]:bg-accent [&.open]:border-accent [&.open]:shadow-[0_2px_6px_rgba(22,21,44,0.18)]";
    let handle_cls = if col_open {
        format!("{COL_HANDLE_BASE} open")
    } else {
        COL_HANDLE_BASE.to_owned()
    };
    let shift = *scroll_x.read();
    let track_style = if shift > 0.0 {
        format!("margin-left: -{shift:.1}px;")
    } else {
        String::new()
    };
    rsx! {
        div {
            id: format!("col-strip-{index}"),
            class: "ledger-col-strip",
            style: track_style,
            for (slot_c, &weight) in weights.iter().enumerate() {
                div {
                    key: "{slot_c}",
                    class: "ledger-col-slot",
                    class: if slot_c == c { "active" },
                    flex_grow: weight.to_string(),
                    if slot_c == c {
                        div {
                            class: "ledger-col-handle-wrap",
                            onpointerenter: move |_| {
                                if !*hovered.peek() { hovered.set(true); }
                                if *hovered_col.peek() != Some(c) { hovered_col.set(Some(c)); }
                            },
                            onpointerdown: move |e| { e.prevent_default(); e.stop_propagation(); },
                            Button {
                                id: format!("col-menu-btn-{index}-{c}"),
                                variant: Variant::Bare,
                                class: handle_cls.clone(),
                                label: "Column menu",
                                tip: "Column options & alignment",
                                icon: crate::Icon::DotsHorizontal,
                                onclick: move |_| {
                                    let target = super::writing::TableMenu::Column(index, c);
                                    let next = (*writing.grid.menu.peek() != Some(target)).then_some(target);
                                    writing.show_table_menu(next);
                                },
                            }
                        }
                        if col_open {
                            Menu {
                                id: format!("col-menu-{index}-{c}"),
                                class: if align_start { "ledger-axis-menu col-menu align-start" } else if align_end { "ledger-axis-menu col-menu align-end" } else { "ledger-axis-menu col-menu" },
                                label: format!("Column {} menu", c + 1),
                                MenuRow {
                                    Button { id: format!("align-left-{index}"), variant: Variant::Icon, label: "Align left", icon: crate::Icon::AlignLeft, selected: matches!(col_align, Alignment::Left | Alignment::None),
                                        onclick: move |_| writing.align_columns(index, col_rect, Alignment::Left) }
                                    Button { id: format!("align-center-{index}"), variant: Variant::Icon, label: "Align center", icon: crate::Icon::AlignCenter, selected: col_align == Alignment::Center,
                                        onclick: move |_| writing.align_columns(index, col_rect, Alignment::Center) }
                                    Button { id: format!("align-right-{index}"), variant: Variant::Icon, label: "Align right", icon: crate::Icon::AlignRight, selected: col_align == Alignment::Right,
                                        onclick: move |_| writing.align_columns(index, col_rect, Alignment::Right) }
                                }
                                MenuSeparator {}
                                MenuItem { id: format!("col-menu-move-left-{index}-{c}"), label: "Move left", badge: "←", disabled: c == 0,
                                    onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.move_column_dir(index, active_row, c, false); } }
                                MenuItem { id: format!("col-menu-move-right-{index}-{c}"), label: "Move right", badge: "→", disabled: c + 1 >= width_count,
                                    onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.move_column_dir(index, active_row, c, true); } }
                                MenuItem { id: format!("col-menu-insert-left-{index}-{c}"), label: "Insert left", badge: "+",
                                    onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.insert_column_at(index, active_row, c); } }
                                MenuItem { id: format!("col-menu-insert-right-{index}-{c}"), label: "Insert right", badge: "+",
                                    onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.insert_column_at(index, active_row, c + 1); } }
                                MenuSeparator {}
                                MenuItem { id: format!("col-menu-delete-{index}-{c}"), label: "Delete column", badge: "×", danger: true, disabled: width_count <= 1,
                                    onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.remove_axis(index, col_rect, true); } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn RowStrip(
    writing: Writing,
    index: usize,
    mut hovered: Signal<bool>,
    mut hovered_col: Signal<Option<usize>>,
    mut hovered_row: Signal<Option<usize>>,
    row_count: usize,
    active_col: usize,
    multi_active: bool,
    open_row: Option<usize>,
) -> Element {
    let hover_r = (*hovered_row.read()).filter(|&r| r > 0 && r < row_count);
    let target_row = if multi_active {
        None
    } else {
        open_row.or(hover_r)
    };
    // Rows grow with wrapped text; until they are measured the stylesheet's fixed heights apply.
    let heights = writing.grid.row_heights.read().get(index).cloned().unwrap_or_default();
    let px = |r: usize, property: &str| {
        heights.get(r).filter(|h| **h > 0.0).map(|h| format!("{property}: {h}px;"))
    };
    rsx! {
        div {
            id: format!("row-strip-{index}"),
            class: "ledger-row-strip",
            style: px(0, "top"),
            for r in 1..row_count {
                {
                    let row_open = open_row == Some(r);
                    let is_target_row = target_row == Some(r);
                    let row_rect = table::CellRect { anchor: [r, active_col], head: [r, active_col] };
                    const ROW_HANDLE_BASE: &str = "ledger-row-handle flex items-center justify-center flex-none w-[15px] h-7 m-0 p-0 border border-solid border-[rgba(202,198,190,0.65)] rounded-full text-faint bg-[rgba(252,251,248,0.76)] shadow-[0_1px_2px_rgba(22,21,44,0.06)] cursor-pointer select-none pointer-events-auto [&_.tool-icon]:w-[13px] [&_.tool-icon]:h-[13px] [&_.tool-icon_svg]:w-[13px] [&_.tool-icon_svg]:h-[13px] hover:text-on-accent hover:bg-accent hover:border-accent hover:shadow-[0_2px_6px_rgba(22,21,44,0.18)] focus-visible:text-on-accent focus-visible:bg-accent focus-visible:border-accent focus-visible:shadow-[0_2px_6px_rgba(22,21,44,0.18)] focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1 [&.open]:text-on-accent [&.open]:bg-accent [&.open]:border-accent [&.open]:shadow-[0_2px_6px_rgba(22,21,44,0.18)]";
                    let row_handle_cls = if row_open {
                        format!("{ROW_HANDLE_BASE} open")
                    } else {
                        ROW_HANDLE_BASE.to_owned()
                    };
                    rsx! {
                        div {
                            key: "{r}",
                            class: "ledger-row-slot",
                            style: px(r, "height"),
                            class: if is_target_row || row_open { "active" },
                            if is_target_row {
                                div {
                                    class: "ledger-row-handle-wrap",
                                    onpointerenter: move |_| {
                                        if !*hovered.peek() { hovered.set(true); }
                                        if *hovered_row.peek() != Some(r) { hovered_row.set(Some(r)); }
                                    },
                                    onpointerdown: move |e| { e.prevent_default(); e.stop_propagation(); },
                                    Button {
                                        id: format!("row-menu-btn-{index}-{r}"),
                                        variant: Variant::Bare,
                                        class: row_handle_cls,
                                        label: "Row menu",
                                        tip: "Row options",
                                        icon: crate::Icon::DotsVertical,
                                        onclick: move |_| {
                                            let target = super::writing::TableMenu::Row(index, r);
                                            let next = (*writing.grid.menu.peek() != Some(target)).then_some(target);
                                            writing.show_table_menu(next);
                                        },
                                    }
                                }
                            }
                            if row_open {
                                Menu {
                                    id: format!("row-menu-{index}-{r}"),
                                    class: "ledger-axis-menu row-menu",
                                    label: format!("Row {} menu", r + 1),
                                    MenuItem { id: format!("row-menu-move-up-{index}-{r}"), label: "Move up", badge: "↑", disabled: r <= 1,
                                        onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.move_row_dir(index, r, active_col, false); } }
                                    MenuItem { id: format!("row-menu-move-down-{index}-{r}"), label: "Move down", badge: "↓", disabled: r + 1 >= row_count,
                                        onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.move_row_dir(index, r, active_col, true); } }
                                    MenuItem { id: format!("row-menu-insert-above-{index}-{r}"), label: "Insert above", badge: "+",
                                        onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.insert_row_at(index, r, active_col); } }
                                    MenuItem { id: format!("row-menu-insert-below-{index}-{r}"), label: "Insert below", badge: "+",
                                        onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.insert_row_at(index, r + 1, active_col); } }
                                    MenuSeparator {}
                                    MenuItem { id: format!("row-menu-delete-{index}-{r}"), label: "Delete row", badge: "×", danger: true,
                                        onclick: move |_| { hovered_col.set(None); hovered_row.set(None); writing.remove_axis(index, row_rect, false); } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GridRow(
    writing: Writing,
    index: usize,
    row: usize,
    last: bool,
    open_col: Option<usize>,
    open_row: Option<usize>,
    mut hovered: Signal<bool>,
    mut hovered_col: Signal<Option<usize>>,
    mut hovered_row: Signal<Option<usize>>,
    values: Vec<(String, &'static str, usize)>,
) -> Element {
    let row_targeted = open_row == Some(row);
    rsx! {
        div { class: "grid-row", class: if last { "ledger-last" }, class: if row_targeted { "row-menu-target" }, role: "row",
            for (col, (value, alignment, weight)) in values.into_iter().enumerate() {
                {
                    let field = Field::Cell(index, row, col);
                    let selected_rect = writing.grid.cells.read().as_ref().filter(|(i,s)| *i==index && s.rows().contains(&row) && s.cols().contains(&col)).map(|(_,s)|*s);
                    let selected = selected_rect.is_some();
                    let active = *writing.active.read() == Some(field) && writing.span.read().is_none();
                    let whole_cell = selected && active && !*writing.grid.editing.read();
                    let col_targeted = open_col == Some(col);
                    let next_row = (row > 0).then_some(row);
                    let mut mark_hovered = move || {
                        if !*hovered.peek() {
                            hovered.set(true);
                        }
                        if *hovered_col.peek() != Some(col) {
                            hovered_col.set(Some(col));
                        }
                        if *hovered_row.peek() != next_row {
                            hovered_row.set(next_row);
                        }
                    };
                    let mut clear_hovered = move || {
                        if hovered_col.peek().is_some() {
                            hovered_col.set(None);
                        }
                        if hovered_row.peek().is_some() {
                            hovered_row.set(None);
                        }
                    };
                    rsx! {
                        div { key: "{col}", id: "slot-{index}-{row}-{col}", class: "cell-slot ledger-cell align-{alignment}",
                            class: if row == 0 { "ledger-head" } else { "ledger-data" },
                            class: if col == 0 && row > 0 && alignment == "left" { "ledger-label" },
                            class: if row > 0 && alignment == "right" { "ledger-number" },
                            class: if selected { "range-cell" }, class: if active { "is-editing" },
                            class: if whole_cell { "range-target" },
                            class: if col_targeted { "col-menu-target" },
                            aria_selected:selected.to_string(), flex_grow: weight.to_string(), role: if row == 0 { "columnheader" } else { "cell" },
                            onpointerenter: move |_| mark_hovered(),
                            onpointermove: move |_| mark_hovered(),
                            div { class: "cell-field",
                                // Long values wrap. A Blitz textarea doesn't grow with its text, so this hidden
                                // copy wraps the same way and gives the cell, and so the row, its height. Its
                                // shaped width also sizes centred and right-aligned frames.
                                span { class: "cell-measure", class: if whole_cell { "range-value" }, aria_hidden: "true",
                                    if value.is_empty() { "\u{a0}" } else { "{value}" }
                                }
                                textarea {
                                    id: field.id(), "data-grid-keys": "true", "data-range-keys": if selected && !*writing.grid.editing.read() { "true" }, class: "grid-cell",
                                    aria_label: format!("Row {}, column {}", row + 1, col + 1), value: "{value}",
                                    onmounted: move |e| {
                                        mounted_field(e, field, writing.focus);
                                        writing.refresh_link_card();
                                    },
                                    onfocus: move |_| writing.track_field(field),
                                    onpointerup: move |_| writing.track_field(field),
                                    onkeydown: move |e| {
                                        let is_modifier = matches!(
                                            e.key(),
                                            Key::Named(
                                                NamedKey::Shift
                                                    | NamedKey::Control
                                                    | NamedKey::Alt
                                                    | NamedKey::Meta
                                            )
                                        );
                                        if !is_modifier {
                                            clear_hovered();
                                        }
                                        writing.cell_key(index, row, col, e);
                                    },
                                    onkeyup: move |_| writing.track_field(field),
                                    oninput: move |e| {
                                        clear_hovered();
                                        writing.cell_input(index, row, col, &e.value());
                                    },
                                    "{value}"
                                }
                                {(writing.ink)().into_iter().filter(|ink| ink.field == field).map(|ink| ink.draw())}
                                crate::comments::CommentInk { writing, field }
                                SelectionInk { writing, field }
                                super::markdown::CaretOverlay { writing, field }
                            }
                            if let Some(s) = selected_rect {
                                span { class:"range-outline", aria_hidden:"true",
                                    style:format!("border-width:{}px {}px {}px {}px;",u8::from(row==s.rows().start),u8::from(col+1==s.cols().end),u8::from(row+1==s.rows().end),u8::from(col==s.cols().start)) }
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
    let mut hovered_col = use_signal(|| None::<usize>);
    let mut hovered_row = use_signal(|| None::<usize>);
    let mut within = use_signal(|| false);
    let mut scroll_x = use_signal(|| 0.0_f64);
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
            .grid.cells
            .read()
            .as_ref()
            .filter(|(i, _)| *i == index)
            .map(|(_, rect)| *rect)
            .unwrap_or(table::CellRect {
                anchor: [r, c],
                head: [r, c],
            })
    });
    let active_menu = *writing.grid.menu.read();
    let menu_in_table = matches!(
        active_menu,
        Some(super::writing::TableMenu::Column(t, _) | super::writing::TableMenu::Row(t, _))
            if t == index
    );
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
    let logical_blocks = writing.logical_blocks.read();
    let table_block_idx = super::blocks::block_index_at(&logical_blocks, grid.range.start);
    let total_blocks = logical_blocks.len();
    drop(logical_blocks);
    let weights_vec = weights.read().clone();
    let alignments_vec = grid.align.clone();
    let width_count = grid.width();
    let row_count = grid.rows.len();
    let active_row = selected.map_or(0, |(r, _)| r);
    let active_col = selected.map_or(0, |(_, c)| c);
    let multi_active = rect.is_some_and(|r| r.count() > 1);
    let open_col = match active_menu {
        Some(super::writing::TableMenu::Column(t, c)) if t == index && c < width_count => Some(c),
        _ => None,
    };
    let open_row = match active_menu {
        Some(super::writing::TableMenu::Row(t, r)) if t == index && r < row_count => Some(r),
        _ => None,
    };
    let dragging_this = table_block_idx.is_some_and(|b| {
        writing
            .handles.drag
            .read()
            .is_some_and(|d| d.dragging && d.from == b)
    });
    rsx! {
        div { id: "grid-{index}", class: "grid-block ledger block-wrap", class: if rect.is_some() || hovered() || within() || menu_in_table { "engaged is-active-block" }, class: if dragging_this { "is-dragging-source" },
            onkeydown:move |e| {
                if menu_in_table && e.key()==Key::Named(NamedKey::Escape) {
                    e.prevent_default(); e.stop_propagation(); writing.grid.menu.set(None);
                    if let Some((r,c))=selected && let Some(root)=writing.root.peek().clone() {
                        let mut doc = root.doc_mut();
                        if let Some(id) = doc.get_element_by_id(&Field::Cell(index,r,c).id()) { doc.set_focus_to(id); }
                    }
                } else if let Some((r, c)) = selected && e.key() == Key::Named(NamedKey::F10) && e.modifiers() == Modifiers::SHIFT {
                    e.prevent_default(); e.stop_propagation();
                    let next = if r == 0 {
                        super::writing::TableMenu::Column(index, c)
                    } else {
                        super::writing::TableMenu::Row(index, r)
                    };
                    writing.show_table_menu(Some(next));
                }
            },
            onpointerenter: move |_| { if !*hovered.peek() { hovered.set(true); } },
            onpointerleave: move |_| {
                if *hovered.peek() { hovered.set(false); }
                if hovered_col.peek().is_some() { hovered_col.set(None); }
                if hovered_row.peek().is_some() { hovered_row.set(None); }
            },
            onfocusin: move |_| { if !*within.peek() { within.set(true); } },
            onfocusout: move |_| { if *within.peek() { within.set(false); } },
            if let Some(bidx) = table_block_idx {
                BlockHandle { writing, block_idx: bidx, total: total_blocks, top: 6 }
            }
            ColStrip { writing, index, hovered, hovered_col, hovered_row, weights: weights_vec.clone(), alignments: alignments_vec, width_count, active_row, multi_active, open_col, scroll_x }
            RowStrip { writing, index, hovered, hovered_col, hovered_row, row_count, active_col, multi_active, open_row }
            div { id: "grid-scroll-{index}", class: "grid-scroll", role: "table", aria_label: "Markdown table",
                onscroll: move |_| {
                    if let Some(handle) = writing.root.peek().as_ref() {
                        let doc = handle.doc();
                        if let Some(node) = doc
                            .get_element_by_id(&format!("grid-scroll-{index}"))
                            .and_then(|id| doc.get_node(id))
                        {
                            let sx = node.scroll_offset().x;
                            if (*scroll_x.peek() - sx).abs() > 0.25 {
                                scroll_x.set(sx);
                            }
                        }
                    }
                    writing.refresh_link_card();
                },
                for (r, _) in grid.rows.iter().enumerate() {
                    GridRow { key: "{r}", writing, index, row: r, last: r + 1 == grid.rows.len(), open_col, open_row, hovered, hovered_col, hovered_row,
                        values: weights_vec.iter().enumerate().map(|(c, &weight)| (writing.projection(Field::Cell(index, r, c)).display, align(grid, c), weight)).collect()
                    }
                }
            }
            div { class: "table-bar ledger-tools",
                if let Some(rect) = rect.filter(|r| r.count() > 1) {
                    {
                        let [r, c] = rect.head;
                        rsx! {
                            Toolbar { id: "table-bar-{index}", class: "ledger-bar-group", label: "Selected cells", look: ToolbarLook::Inline, keep_focus: true, contain: true,
                                span { class: "ledger-bar-scope", "{scope_label(rect)}" }
                                span { class: "cell-count", "{rect.count()} cells" }
                                ToolbarSep {}
                                Button { id: format!("clear-cells-{index}"), variant: Variant::Ghost, class: "ledger-chip", label: "Clear selected cells", text: "Clear",
                                    onclick: move |_| { writing.clear_or_fill(index, rect, "", r, c); } }
                                Button { id: format!("delete-row-{index}"), variant: Variant::Ghost, class: "ledger-chip", danger: true, label: row_label, text: "Del rows",
                                    disabled: rect.rows().start == 0, onclick: move |_| writing.remove_axis(index, rect, false) }
                                Button { id: format!("delete-column-{index}"), variant: Variant::Ghost, class: "ledger-chip", danger: true, label: col_label, text: "Del cols",
                                    disabled: rect.cols().len() >= grid.width(), onclick: move |_| writing.remove_axis(index, rect, true) }
                            }
                        }
                    }
                }
            }
        }
    }
}
