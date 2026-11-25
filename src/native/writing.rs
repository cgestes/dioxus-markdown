use super::document::{self, Block, BlockKind, BodyStart};
use super::marker_view::MarkerBox;
use super::table::{self, Table};
use super::{ByteSelection, Field, FocusRequest, NodeHandle, Sections, report, selection};
use dioxus_native::prelude::*;
use margin_editor_core::checkbox::{Input, Marker, Mode};
use margin_editor_core::note::{Note, text_change};
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TableMenu {
    Column(usize, usize),
    Row(usize, usize),
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct BlockDrag {
    pub from: usize,
    pub start_y: f64,
    pub dragging: bool,
    pub target: usize,
    pub indicator_y: Option<f64>,
}

pub(super) type TrackedKey = (
    Field,
    ByteSelection,
    u64,
    super::projection::EditMode,
    Option<(usize, usize)>,
    Option<usize>,
);

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Writing {
    pub personal: Signal<Option<super::personal::Session>>,
    pub note: Signal<Note>,
    pub root: Signal<Option<NodeHandle>>,
    pub focus: FocusRequest,
    pub message: Signal<String>,
    pub contents: Memo<String>,
    pub doc_rev: Memo<u64>,
    pub markers: Memo<Vec<Marker>>,
    pub tables: Memo<Vec<Table>>,
    pub blocks: Memo<Vec<Block>>,
    pub logical_blocks: Memo<Vec<Range<usize>>>,
    pub goal_x: Signal<Option<f64>>,
    // A selection that extends beyond the focused input, in Markdown byte offsets.
    pub span: Signal<Option<ByteSelection>>,
    pub cells: Signal<Option<(usize, table::CellRect)>>,
    pub cell_editing: Signal<bool>,
    pub table_menu: Signal<Option<TableMenu>>,
    pub quiet_cell: Signal<Option<Field>>,
    pub selected_text: Signal<Option<ByteSelection>>,
    pub format_position: Signal<Option<[i32; 2]>>,
    pub pressed_box: Signal<Option<usize>>,
    pub mouse_box: Signal<Option<usize>>,
    pub edit_mode: Signal<super::projection::EditMode>,
    pub cursor_source: Signal<Option<ByteSelection>>,
    pub drag: Signal<Option<super::pointer::Drag>>,
    pub parsed: Memo<super::projection::ParsedMarkdown>,
    pub dom_dirty: Signal<bool>,
    pub last_tracked: Signal<Option<TrackedKey>>,
    pub empty_line: Signal<Option<(Field, f32, f32)>>,
    pub slash_menu: Signal<Option<super::slash::SlashState>>,
    pub slash_selected: Signal<usize>,
    pub block_menu: Signal<Option<usize>>,
    pub block_drag: Signal<Option<BlockDrag>>,

    pub draft_table: Signal<Option<usize>>,
    pub heights: Signal<Vec<usize>>,
    pub heading_heights: Signal<Vec<usize>>,
    pub caret_line_col: Signal<Option<[usize; 2]>>,
    pub syntax: Memo<Vec<super::markdown::Token>>,
    pub ink: Signal<Vec<super::markdown::Ink>>,
    pub caret: Signal<Option<super::markdown::CaretInk>>,
    pub active: Signal<Option<Field>>,
    // Keep the origin of an open body stable when its first text becomes a heading.
    pub body: Signal<Option<BodyStart>>,
    pub parked_selection: Signal<Option<ByteSelection>>,
    pub marker: Signal<Option<usize>>,
    pub boxes: Signal<Vec<MarkerBox>>,
    pub literal: Signal<Option<(usize, usize)>>,
    pub comments: Signal<Vec<super::comments::CommentThread>>,
    pub comments_open: Signal<bool>,
    pub active_comment: Signal<Option<usize>>,
    pub draft_comment: Signal<Option<super::comments::DraftComment>>,
    /// Who answers comments: the page's agent, or the local stand-in.
    pub comment_agent: Signal<super::comments::CommentAgent>,
    /// The page's comment file in the space, when the notebook edits a page.
    pub comment_file: Signal<super::comments::CommentFile>,
    /// Replies still arriving, by message id; only their bubbles re-render.
    pub comment_stream: Signal<std::collections::HashMap<String, String>>,
    /// Where every thread is now, and the text under it.
    pub comment_ranges: Memo<Vec<super::comments::Placed>>,
    /// The page this notebook edits, if any, and where it stands.
    pub page: Signal<Option<super::page::PageSource>>,
    pub page_state: Signal<super::page::PageState>,
}

impl Writing {
    pub fn sections(self, md: &str) -> Sections {
        match *self.body.read() {
            Some(origin) => Sections::with_origin(md, origin),
            None => Sections::read(md),
        }
    }

    pub fn range(self, field: Field, md: &str) -> Range<usize> {
        let parts = self.sections(md);
        match field {
            Field::Title => match (parts.hashes, parts.title) {
                (Some(h), Some(t)) => h.start..t.end,
                (_, Some(t)) => t,
                _ => 0..0,
            },
            Field::TitleGap => {
                let start = Sections::with_origin(md, BodyStart::AfterHeading).body;
                start..start
            }
            Field::Heading(i) => self
                .blocks
                .read()
                .get(i)
                .and_then(|b| match &b.kind {
                    BlockKind::Heading { hashes, title } => Some(hashes.start..title.end),
                    _ => None,
                })
                .unwrap_or(md.len()..md.len()),
            Field::Body(i) => {
                document::gap(md, parts.body, &self.blocks.read(), i).unwrap_or(md.len()..md.len())
            }
            Field::Cell(t, r, c) => self
                .tables
                .read()
                .get(t)
                .filter(|t| r < t.rows.len() && c < t.width())
                .map_or(md.len()..md.len(), |t| t.cell(r, c)),
            _ => unreachable!("a writing field is the title or body"),
        }
    }

    pub fn heading(self, field: Field, md: &str) -> Option<(Range<usize>, Range<usize>)> {
        match field {
            Field::Title => {
                let parts = self.sections(md);
                Some((parts.hashes?, parts.title?))
            }
            Field::Heading(i) => match &self.blocks.read().get(i)?.kind {
                BlockKind::Heading { hashes, title } => Some((hashes.clone(), title.clone())),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn field_at(self, pos: usize) -> Field {
        let md = self.contents.read();
        let parts = self.sections(&md);
        if parts.title.is_some() && pos <= md.find(['\r', '\n']).unwrap_or(md.len()) {
            return Field::Title;
        }
        if pos < parts.body && parts.origin == BodyStart::AfterBlankLine {
            return Field::TitleGap;
        }
        self.tables.read();
        let blocks = self.blocks.read();
        let i = blocks.partition_point(|b| b.line.end < pos);
        if let Some(b) = blocks.get(i)
            && b.line.start <= pos
        {
            match b.kind {
                BlockKind::Heading { .. } => Field::Heading(i),
                BlockKind::Table(t) => {
                    let (r, c) = self.tables.read()[t].position(pos);
                    Field::Cell(t, r, c)
                }
            }
        } else {
            Field::Body(i)
        }
    }

    pub fn clear_goal(mut self) {
        self.goal_x.set(None);
    }
    pub fn pointer(self, field: Field) {
        self.clear_goal();
        self.track_field(field);
    }

    pub fn resolve_dom(self, handle: &NodeHandle) {
        if *self.dom_dirty.peek() {
            *self.dom_dirty.write_unchecked() = false;
            handle.doc_mut().resolve(0.0);
        }
    }

    pub fn projection(self, field: Field) -> super::projection::Projection {
        let md = self.contents.read();
        let parsed = self.parsed.read();
        let range = self.range(field, &md);
        let markers = self.markers.read();
        let mode = *self.edit_mode.read();
        let is_heading = matches!(field, Field::Title | Field::Heading(_));
        let drag = *self.drag.read();
        let active_is_field = *self.active.read() == Some(field);
        let span_covers = self.span.read().is_some_and(|[a, b]| {
            let (lo, hi) = (a.min(b), a.max(b));
            lo < range.end && hi > range.start
        });
        let (active_sel, mouse_box, literal) = if mode == super::projection::EditMode::Markdown {
            if is_heading {
                let dragging_from_other = drag.is_some_and(|d| d.field != field);
                let show_prefix = !dragging_from_other
                    && (active_is_field
                        || span_covers
                        || drag.is_some_and(|d| d.field == field));
                if show_prefix {
                    (
                        (*self.cursor_source.read()).or(Some([range.start; 2])),
                        *self.mouse_box.read(),
                        *self.literal.read(),
                    )
                } else {
                    (None, None, None)
                }
            } else if active_is_field {
                (
                    *self.cursor_source.read(),
                    *self.mouse_box.read(),
                    *self.literal.read(),
                )
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        };
        super::projection::Projection::build(
            &md,
            &parsed,
            range,
            &markers,
            mode,
            active_sel,
            mouse_box,
            literal,
            matches!(field, Field::Body(_)),
            matches!(field, Field::Cell(..)),
        )
    }

    pub fn projection_peek(self, field: Field) -> super::projection::Projection {
        let md = self.contents.read();
        let parsed = self.parsed.read();
        let range = self.range(field, &md);
        let markers = self.markers.read();
        let mode = *self.edit_mode.peek();
        let is_heading = matches!(field, Field::Title | Field::Heading(_));
        let drag = *self.drag.peek();
        let active_is_field = *self.active.peek() == Some(field);
        let span_covers = self.span.peek().is_some_and(|[a, b]| {
            let (lo, hi) = (a.min(b), a.max(b));
            lo < range.end && hi > range.start
        });
        let (active_sel, mouse_box, literal) = if mode == super::projection::EditMode::Markdown {
            if is_heading {
                let dragging_from_other = drag.is_some_and(|d| d.field != field);
                let show_prefix = !dragging_from_other
                    && (active_is_field
                        || span_covers
                        || drag.is_some_and(|d| d.field == field));
                if show_prefix {
                    (
                        (*self.cursor_source.peek()).or(Some([range.start; 2])),
                        *self.mouse_box.peek(),
                        *self.literal.peek(),
                    )
                } else {
                    (None, None, None)
                }
            } else if active_is_field {
                (
                    *self.cursor_source.peek(),
                    *self.mouse_box.peek(),
                    *self.literal.peek(),
                )
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        };
        super::projection::Projection::build(
            &md,
            &parsed,
            range,
            &markers,
            mode,
            active_sel,
            mouse_box,
            literal,
            matches!(field, Field::Body(_)),
            matches!(field, Field::Cell(..)),
        )
    }

    pub fn refresh_empty_line(mut self) {
        let next = self.compute_empty_line();
        if *self.empty_line.peek() != next {
            self.empty_line.set(next);
        }
    }

    fn compute_empty_line(self) -> Option<(Field, f32, f32)> {
        if self.span.peek().is_some()
            || self.cells.peek().is_some()
            || self.drag.peek().is_some()
            || self.selected_text.peek().is_some()
            || self.marker.peek().is_some()
        {
            return None;
        }
        let field = (*self.active.peek())?;
        if !matches!(field, Field::Body(_) | Field::TitleGap) {
            return None;
        }
        if field == Field::TitleGap {
            return Some((field, 18.0, -1.0));
        }
        let [a, b] = self.absolute_selection()?;
        if a != b {
            return None;
        }
        let md = self.contents.read();
        let head = md.floor_char_boundary(b.min(md.len()));
        let line_start = md[..head].rfind('\n').map_or(0, |i| i + 1);
        let line_end = head + md[head..].find('\n').unwrap_or(md.len() - head);
        if !md[line_start..line_end].trim_end_matches('\r').is_empty() {
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
        let caret = input.editor.raw_selection().focus().geometry(layout, 0.0);
        let scale = layout.scale();
        let l = node.final_layout();
        let left = (l.border.left + l.padding.left + caret.x0 as f32 / scale + 18.0).round();
        let line_h = ((caret.y1 - caret.y0) as f32 / scale).max(24.0);
        let top = (l.border.top + l.padding.top + caret.y0 as f32 / scale - input.scroll_offset
            + (line_h - 24.0) / 2.0)
            .round();
        if top < -4.0 || top > l.content_box_height() + 4.0 {
            return None;
        }
        Some((field, left, top))
    }

    pub fn reset(mut self) {
        self.clear_cells();
        self.selected_text.set(None);
        self.empty_line.set(None);
        self.slash_menu.set(None);
        self.slash_selected.set(0);
        self.block_menu.set(None);
        self.block_drag.set(None);
        self.draft_comment.set(None);
        self.caret.set(None);
        self.table_menu.set(None);
        self.mouse_box.set(None);
        *self.last_tracked.write_unchecked() = None;
        self.cursor_source.set(None);
        self.span.set(None);
        self.drag.set(None);
        self.draft_table.set(None);
        self.active.set(None);
        self.parked_selection.set(None);
        self.body.set(None);
        self.marker.set(None);
        self.refresh_boxes();
        self.literal.set(None);
    }

    pub(super) fn marker_at(self, selection: ByteSelection) -> Option<usize> {
        let [a, b] = selection;
        let (lo, hi) = (a.min(b), a.max(b));
        if let Some((open, close)) = *self.literal.peek()
            && open < lo
            && hi <= close
        {
            return Some(open);
        }
        let markers = self.markers.peek();
        let index = markers.partition_point(|m| m.open < lo).checked_sub(1)?;
        let open = markers[index].open;
        (hi <= open + 2).then_some(open)
    }

    pub(super) fn track(mut self, selected: ByteSelection) {
        let md = self.contents.read();
        let head = md.floor_char_boundary(selected[1].min(md.len()));
        let before = &md[..head];
        let from = before.rfind('\n').map_or(0, |n| n + 1);
        let lc = Some([
            before.bytes().filter(|b| *b == b'\n').count() + 1,
            before[from..].chars().count() + 1,
        ]);
        drop(md);
        if *self.caret_line_col.peek() != lc {
            self.caret_line_col.set(lc);
        }
        let text = (selected[0] != selected[1]).then_some(selected);
        if *self.selected_text.peek() != text {
            self.selected_text.set(text);
        }
        self.refresh_format_position();
        let marker = self.marker_at(selected);
        if self
            .literal
            .peek()
            .is_some_and(|(open, _)| Some(open) != marker)
        {
            self.literal.set(None);
        }
        if *self.marker.peek() != marker {
            self.marker.set(marker);
        }
        self.refresh_empty_line();
        self.refresh_slash();
        self.sync_active_comment_at(selected[1]);
    }

    pub fn track_field(mut self, field: Field) {
        let rev = *self.doc_rev.read();
        if self.focus.peek().is_none()
            && *self.active.peek() == Some(field)
            && self.draft_table.peek().is_none()
            && self.drag.peek().is_none()
            && let Some(raw_sel) = selection(self.root, field)
        {
            let key = (
                field,
                raw_sel,
                rev,
                *self.edit_mode.peek(),
                *self.literal.peek(),
                *self.mouse_box.peek(),
            );
            if *self.last_tracked.peek() == Some(key) {
                return;
            }
            *self.last_tracked.write_unchecked() = Some(key);
        } else {
            *self.last_tracked.write_unchecked() = None;
        }
        self.keep_cells_in(field);
        if *self.active.peek() != Some(field) {
            self.literal.set(None);
            self.parked_selection.set(None);
            self.active.set(Some(field));
        }
        if !matches!(field, Field::Body(_)) && self.body.peek().is_some() {
            self.body.set(None);
        } else if matches!(field, Field::Body(_)) && self.body.peek().is_none() {
            let origin = Sections::read(&self.contents.read()).origin;
            self.body.set(Some(origin));
        }
        let draft = *self.draft_table.peek();
        if let Some(at) = draft {
            let md = self.contents.read();
            let delimiter = table::delimiter_line(&md, at);
            let pos = self.absolute_selection().map(|s| s[1]);
            if !matches!(field, Field::Body(_)) || !pos.is_some_and(|p| delimiter.contains(&p)) {
                self.draft_table.set(None);
                if matches!(field, Field::Body(_))
                    && let Some(pos) = pos
                {
                    let target = self.field_at(pos);
                    if target != field {
                        self.open(target, [pos; 2]);
                        return;
                    }
                }
            }
        }
        if self.focus.peek().is_none()
            && let Some(range) = self.absolute_selection()
        {
            let old_display = self.projection_peek(field).display;
            if self.drag.peek().is_none() && *self.cursor_source.peek() != Some(range) {
                self.cursor_source.set(Some(range));
                let next_proj = self.projection_peek(field);
                if next_proj.display != old_display && self.span.peek().is_none() {
                    let local = [
                        next_proj.source_to_display(range[0]),
                        next_proj.source_to_display(range[1]),
                    ];
                    self.focus.set(Some((field, local)));
                }
            }
            self.note.peek().remember_selection(range);
            self.track(range);
        }
        self.refresh_boxes();
    }

    pub fn toggle_literal(mut self) {
        if let (Some(open), Some(field)) = (*self.marker.peek(), *self.active.peek()) {
            let next = self.literal.peek().is_none().then_some((open, open + 2));
            self.literal.set(next);
            if let Some(range) = selection(self.root, field) {
                self.focus.set(Some((field, range)));
            }
        }
    }

    pub fn keep_literal(mut self) {
        let (Some((open, _)), Some(field)) = (*self.literal.peek(), *self.active.peek()) else {
            return;
        };
        let range = selection(self.root, field);
        report(self.note.write().replace(open..open, "\\"), self.message);
        self.literal.set(None);
        self.marker.set(None);
        if let Some(range) = range {
            self.focus.set(Some((field, range.map(|i| i + 1))));
        }
    }

    /// Cmd/Ctrl+Enter toggles the checkbox on the current line (or adds `- [ ] `).
    pub fn toggle_line_task(mut self, field: Field) -> bool {
        if !matches!(field, Field::Body(_) | Field::TitleGap) {
            return false;
        }
        let Some(selected @ [anchor, head]) = self.absolute_selection() else {
            return false;
        };
        let md = self.contents.read().clone();
        if field == Field::TitleGap {
            let eol = table::eol(&md);
            let insert = format!("{eol}- [ ] ");
            let caret = head + insert.len();
            self.note.peek().remember_selection(selected);
            let result = self.note.write().replace(head..head, &insert);
            if result.is_ok() {
                self.body.set(None);
                self.open(self.field_at(caret), [caret; 2]);
            }
            report(result, self.message);
            return true;
        }
        let line_start = md[..head].rfind('\n').map_or(0, |i| i + 1);
        let line_end = head + md[head..].find('\n').unwrap_or(md.len() - head);
        let marker = self
            .markers
            .read()
            .iter()
            .find(|m| m.open >= line_start && m.open + 3 <= line_end)
            .copied();
        if let Some(m) = marker {
            self.note.peek().remember_selection(selected);
            let inside = m.open + 1;
            report(
                self.note
                    .write()
                    .edit_marker(inside..inside, Mode::Structured, Input::Type(' ')),
                self.message,
            );
            self.open(field, selected);
            return true;
        }
        let line = &md[line_start..line_end];
        let trimmed = line.trim_start_matches([' ', '\t']);
        let indent = line.len() - trimmed.len();
        let (insert_at, insert_text) = if trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
        {
            (line_start + indent + 2, "[ ] ")
        } else {
            (line_start + indent, "- [ ] ")
        };
        self.note.peek().remember_selection(selected);
        let delta = insert_text.len();
        let result = self
            .note
            .write()
            .replace(insert_at..insert_at, insert_text);
        if result.is_ok() {
            let shift = |p: usize| if p >= insert_at { p + delta } else { p };
            self.open(field, [shift(anchor), shift(head)]);
        }
        report(result, self.message);
        true
    }

    /// Set the Markdown ATX heading level (1..=6) or convert back to paragraph (0).
    pub fn set_heading_level(mut self, field: Field, level: usize) -> bool {
        if matches!(field, Field::Cell(..) | Field::Source) {
            return false;
        }
        let md = self.contents.read().clone();
        let Some(selected @ [anchor, head]) = self.absolute_selection() else {
            return false;
        };
        if let Some((hashes, title)) = self.heading(field, &md) {
            let current = hashes.len();
            if level == current {
                return true;
            }
            self.note.peek().remember_selection(selected);
            if level == 0 {
                let removed = title.start - hashes.start;
                let result = self.note.write().replace(hashes.start..title.start, "");
                if result.is_ok() {
                    let map = |p: usize| p.saturating_sub(removed);
                    let pos = [map(anchor), map(head)];
                    self.open(self.field_at(pos[1]), pos);
                }
                report(result, self.message);
            } else {
                let replacement = "#".repeat(level.clamp(1, 6));
                let delta = replacement.len() as isize - hashes.len() as isize;
                let result = self.note.write().replace(hashes.clone(), &replacement);
                if result.is_ok() {
                    let map = |p: usize| {
                        if p >= hashes.end {
                            p.saturating_add_signed(delta)
                        } else {
                            p
                        }
                    };
                    let pos = [map(anchor), map(head)];
                    self.open(self.field_at(pos[1]), pos);
                }
                report(result, self.message);
            }
            return true;
        }
        if level == 0 {
            return false;
        }
        let line_start = md[..head].rfind('\n').map_or(0, |i| i + 1);
        let prefix = format!("{} ", "#".repeat(level.clamp(1, 6)));
        let delta = prefix.len();
        self.note.peek().remember_selection(selected);
        let result = self.note.write().replace(line_start..line_start, &prefix);
        if result.is_ok() {
            let map = |p: usize| if p >= line_start { p + delta } else { p };
            let pos = [map(anchor), map(head)];
            self.body.set(None);
            self.open(self.field_at(pos[1]), pos);
        }
        report(result, self.message);
        true
    }

    // The editable title starts after the Markdown separator. Prefix commands
    // operate on its source bytes while the same native input keeps the caret.
    fn title_prefix(mut self, field: Field, input: &str, backspace: bool) -> bool {
        let md = self.contents.read().clone();
        let grow = input.bytes().take_while(|b| *b == b'#').count();
        let Some((hashes, title)) = self.heading(field, &md) else {
            if backspace
                || !(grow > 0 && (grow == input.len() || &input[grow..] == " ") || input == " ")
            {
                return false;
            }
            let eol = table::eol(&md);
            if *self.edit_mode.peek() == super::projection::EditMode::Markdown {
                let prefix = if grow > 0 && grow == input.len() {
                    format!("{}{eol}{eol}", "#".repeat(grow.clamp(1, 6)))
                } else {
                    format!("{} {eol}{eol}", "#".repeat(grow.clamp(1, 6)))
                };
                let caret = prefix.len() - eol.len() * 2;
                report(self.note.write().replace(0..0, &prefix), self.message);
                self.open(Field::Title, [caret; 2]);
            } else {
                let prefix = format!("{} {eol}{eol}", "#".repeat(grow.clamp(1, 6)));
                report(self.note.write().replace(0..0, &prefix), self.message);
            }
            return true;
        };
        let count = hashes.len();
        let next = if input == " " {
            if title.start == hashes.end {
                report(
                    self.note.write().replace(hashes.end..hashes.end, " "),
                    self.message,
                );
            }
            return true;
        } else if backspace {
            count - 1
        } else if grow > 0 && grow == input.len() {
            (count + grow).min(6)
        } else if (1..=6).contains(&grow) && &input[grow..] == " " {
            grow
        } else {
            return false;
        };
        if next == 0 {
            report(
                self.note.write().replace(hashes.start..title.start, ""),
                self.message,
            );
            self.open(self.field_at(hashes.start), [hashes.start; 2]);
        } else if next != count {
            report(
                self.note.write().replace(hashes, &"#".repeat(next)),
                self.message,
            );
        }
        true
    }

    pub fn toggle_line_prefix(mut self, prefix: &str) {
        let Some(field) = *self.active.peek() else {
            return;
        };
        if !matches!(field, Field::Body(_) | Field::TitleGap) {
            return;
        }
        let Some(selected @ [anchor, head]) = self.absolute_selection() else {
            return;
        };
        let md = self.contents.read().clone();
        if field == Field::TitleGap {
            let eol = table::eol(&md);
            let insert = format!("{eol}{prefix}");
            let caret = head + insert.len();
            self.note.peek().remember_selection(selected);
            let result = self.note.write().replace(head..head, &insert);
            if result.is_ok() {
                self.body.set(None);
                self.open(self.field_at(caret), [caret; 2]);
            }
            report(result, self.message);
            return;
        }
        let line_start = md[..head].rfind('\n').map_or(0, |i| i + 1);
        let line_end = head + md[head..].find('\n').unwrap_or(md.len() - head);
        let line = &md[line_start..line_end];
        let trimmed = line.trim_start_matches([' ', '\t']);
        let indent = line.len() - trimmed.len();
        let at = line_start + indent;
        self.note.peek().remember_selection(selected);
        let (range, replacement, delta): (Range<usize>, &str, isize) =
            if trimmed.starts_with(prefix) {
                (at..at + prefix.len(), "", -(prefix.len() as isize))
            } else {
                (at..at, prefix, prefix.len() as isize)
            };
        let result = self.note.write().replace(range, replacement);
        if result.is_ok() {
            let map = |p: usize| {
                if p >= at {
                    p.saturating_add_signed(delta)
                } else {
                    p
                }
            };
            self.open(field, [map(anchor), map(head)]);
        }
        report(result, self.message);
    }

    pub fn insert_rule(mut self) {
        let md = self.contents.read().clone();
        let pos = self
            .absolute_selection()
            .or(*self.parked_selection.peek())
            .map_or(md.len(), |s| s[0].max(s[1]));
        let start = md[..pos].rfind('\n').map_or(0, |i| i + 1);
        let end = pos + md[pos..].find('\n').unwrap_or(md.len() - pos);
        let eol = table::eol(&md);
        let on_empty = md[start..end].trim().is_empty();
        let insert = if on_empty {
            format!("---{eol}{eol}")
        } else {
            format!("{eol}{eol}---{eol}{eol}")
        };
        let caret = end + insert.len();
        let result = self.note.write().replace(end..end, &insert);
        if result.is_ok() {
            self.open(self.field_at(caret), [caret; 2]);
        }
        report(result, self.message);
    }

    pub fn insert_code_block(mut self) {
        let md = self.contents.read().clone();
        let pos = self
            .absolute_selection()
            .or(*self.parked_selection.peek())
            .map_or(md.len(), |s| s[0].max(s[1]));
        let start = md[..pos].rfind('\n').map_or(0, |i| i + 1);
        let end = pos + md[pos..].find('\n').unwrap_or(md.len() - pos);
        let eol = table::eol(&md);
        let on_empty = md[start..end].trim().is_empty();
        let (insert, caret) = if on_empty {
            (format!("```{eol}{eol}```"), end + 3 + eol.len())
        } else {
            (
                format!("{eol}{eol}```{eol}{eol}```"),
                end + eol.len() * 2 + 3 + eol.len(),
            )
        };
        let result = self.note.write().replace(end..end, &insert);
        if result.is_ok() {
            self.open(self.field_at(caret), [caret; 2]);
        }
        report(result, self.message);
    }

    pub fn insert_image(mut self) {
        let md = self.contents.read().clone();
        let pos = self
            .absolute_selection()
            .or(*self.parked_selection.peek())
            .map_or(md.len(), |s| s[0].max(s[1]));
        let snippet = "![alt text](https://)";
        let result = self.note.write().replace(pos..pos, snippet);
        if result.is_ok() {
            let sel = [pos + 2, pos + 2 + "alt text".len()];
            self.open(self.field_at(sel[1]), sel);
        }
        report(result, self.message);
    }

    pub fn move_block_dir(mut self, block_idx: usize, down: bool) {
        self.block_menu.set(None);
        let md = self.contents.read().clone();
        let list = super::blocks::split_blocks(&md);
        let Some((splices, caret)) = super::blocks::move_block(&md, &list, block_idx, down) else {
            return;
        };
        if let Some(sel) = self.absolute_selection() {
            self.note.peek().remember_selection(sel);
        }
        let result = self.note.write().splice(&splices);
        if result.is_ok() {
            self.body.set(None);
            self.open(self.field_at(caret), [caret; 2]);
        }
        report(result, self.message);
    }

    pub fn move_block_to_slot(mut self, from: usize, to_slot: usize) {
        self.block_menu.set(None);
        let md = self.contents.read().clone();
        let list = super::blocks::split_blocks(&md);
        let Some((splices, caret)) = super::blocks::move_block_to(&md, &list, from, to_slot) else {
            return;
        };
        if let Some(sel) = self.absolute_selection() {
            self.note.peek().remember_selection(sel);
        }
        let result = self.note.write().splice(&splices);
        if result.is_ok() {
            self.body.set(None);
            self.open(self.field_at(caret), [caret; 2]);
        }
        report(result, self.message);
    }

    pub fn start_block_drag(mut self, block_idx: usize, client_y: f64) {
        self.end_drag();
        self.table_menu.set(None);
        self.block_drag.set(Some(BlockDrag {
            from: block_idx,
            start_y: client_y,
            dragging: false,
            target: block_idx,
            indicator_y: None,
        }));
    }

    pub fn duplicate_block_at(mut self, block_idx: usize) {
        self.block_menu.set(None);
        let md = self.contents.read().clone();
        let list = super::blocks::split_blocks(&md);
        let Some((splices, caret)) = super::blocks::duplicate_block(&md, &list, block_idx) else {
            return;
        };
        if let Some(sel) = self.absolute_selection() {
            self.note.peek().remember_selection(sel);
        }
        let result = self.note.write().splice(&splices);
        if result.is_ok() {
            self.body.set(None);
            self.open(self.field_at(caret), [caret; 2]);
        }
        report(result, self.message);
    }

    pub fn delete_block_at(mut self, block_idx: usize) {
        self.block_menu.set(None);
        let md = self.contents.read().clone();
        let list = super::blocks::split_blocks(&md);
        let Some((splices, caret)) = super::blocks::delete_block(&md, &list, block_idx) else {
            return;
        };
        if let Some(sel) = self.absolute_selection() {
            self.note.peek().remember_selection(sel);
        }
        let result = self.note.write().splice(&splices);
        if result.is_ok() {
            self.body.set(None);
            self.open(self.field_at(caret), [caret; 2]);
        }
        report(result, self.message);
    }

    pub fn key(mut self, field: Field, event: KeyboardEvent) {
        if !event.is_composing() && self.slash_key(&event) {
            return;
        }
        // Only clear `mouse_box` on navigation/editing keys that reveal or inspect the marker;
        // clearing it on printable character KeyDown would shift `projection_peek` ahead of the
        // native textarea buffer before `oninput` maps the typed character.
        if self.mouse_box.peek().is_some()
            && matches!(
                event.key(),
                Key::Named(
                    NamedKey::ArrowLeft
                        | NamedKey::ArrowRight
                        | NamedKey::ArrowUp
                        | NamedKey::ArrowDown
                        | NamedKey::Home
                        | NamedKey::End
                        | NamedKey::Backspace
                        | NamedKey::Delete
                        | NamedKey::F2
                )
            )
        {
            self.mouse_box.set(None);
        }
        self.track_field(field);
        if event.is_composing() {
            return;
        }
        if event.key() == Key::Named(NamedKey::Escape) && self.block_menu.peek().is_some() {
            event.prevent_default();
            self.block_menu.set(None);
            return;
        }
        if self.document_key(field, &event)
            || self.span.peek().is_some() && matches!(event.key(), Key::Character(_))
        {
            return;
        }
        let key = event.key();
        if matches!(field, Field::Body(_))
            && event.modifiers().is_empty()
            && matches!(
                key,
                Key::Named(NamedKey::Enter | NamedKey::Tab | NamedKey::Escape)
            )
            && self.finish_table_line(key == Key::Named(NamedKey::Enter))
        {
            event.prevent_default();
            return;
        }
        if (event.modifiers() - Modifiers::SHIFT).is_empty()
            && matches!(key, Key::Named(NamedKey::ArrowDown | NamedKey::ArrowUp))
        {
            if self.vertical(
                field,
                key == Key::Named(NamedKey::ArrowDown),
                event.modifiers().shift(),
            ) {
                event.prevent_default();
                event.stop_propagation();
            }
            return;
        }
        self.clear_goal();
        if field == Field::TitleGap && event.modifiers().is_empty() {
            let md = self.contents.read().clone();
            let start = self.range(field, &md).start;
            let gap = table::eol(&md);
            let change = match key {
                Key::Named(NamedKey::Enter) => Some((start..start, gap, start + gap.len())),
                Key::Named(NamedKey::Backspace) => {
                    let before = if md[..start].ends_with("\r\n") { 2 } else { 1 };
                    Some((start - before..start, "", start - before))
                }
                Key::Named(NamedKey::Delete) => Some((
                    start..start + document::newline_len(&md[start..]),
                    "",
                    start,
                )),
                _ => None,
            };
            if let Some((range, text, pos)) = change {
                event.prevent_default();
                report(self.note.write().replace(range, text), self.message);
                self.open(self.field_at(pos), [pos; 2]);
                return;
            }
        }
        let hidden_heading_prefix = matches!(field, Field::Title | Field::Heading(_))
            && (*self.edit_mode.peek() == super::projection::EditMode::Rich
                || (field == Field::Title && Sections::read(&self.contents.read()).title.is_none()));
        if hidden_heading_prefix
            && !event
                .modifiers()
                .intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT)
            && selection(self.root, field) == Some([0; 2])
        {
            let cmd = match &key {
                Key::Character(s) => Some(s.as_str()),
                Key::Named(NamedKey::Backspace) => Some(""),
                _ => None,
            };
            if cmd.is_some_and(|cmd| {
                self.title_prefix(field, cmd, key == Key::Named(NamedKey::Backspace))
            }) {
                event.prevent_default();
                return;
            }
        }
        let guarded = self.marker.peek().is_some() && self.literal.peek().is_none();
        if event
            .modifiers()
            .intersects(Modifiers::CONTROL | Modifiers::META | Modifiers::ALT)
        {
            if guarded
                && matches!(key, Key::Character(s) if s.eq_ignore_ascii_case("v") || s.eq_ignore_ascii_case("x"))
            {
                event.prevent_default();
            }
            return;
        }
        if !guarded
            && matches!(field, Field::Body(_))
            && let Some(range) = self.absolute_selection()
        {
            use super::lists::{self, Command};
            let command = match key {
                Key::Named(NamedKey::Enter) if event.modifiers().is_empty() => {
                    Some(Command::Continue)
                }
                Key::Named(NamedKey::Backspace) if event.modifiers().is_empty() => {
                    Some(Command::Backspace)
                }
                Key::Named(NamedKey::Tab) => {
                    Some(if event.modifiers().contains(Modifiers::SHIFT) {
                        Command::Outdent
                    } else {
                        Command::Indent
                    })
                }
                _ => None,
            };
            if let Some(edit) =
                command.and_then(|cmd| lists::edit(&self.contents.read(), range, cmd))
            {
                event.prevent_default();
                report(self.note.write().splice(&edit.changes), self.message);
                self.open(field, edit.caret);
                return;
            }
        }
        if key == Key::Named(NamedKey::F2) && self.marker.peek().is_some() {
            event.prevent_default();
            if !event.is_auto_repeating() {
                self.toggle_literal();
            }
            return;
        }
        if key == Key::Named(NamedKey::Escape) {
            if self.literal.peek().is_some() {
                event.prevent_default();
                self.literal.set(None);
                self.track_field(field);
            } else if matches!(field, Field::Body(_)) {
                event.prevent_default();
                let range = self.absolute_selection();
                self.active.set(None);
                self.marker.set(None);
                self.caret.set(None);
                self.refresh_boxes();
                self.parked_selection.set(range);
                if let Some(handle) = self.root.peek().as_ref() {
                    let mut doc = handle.doc_mut();
                    if let Some(id) = doc.get_element_by_id("body-surface") {
                        doc.set_focus_to(id);
                    }
                }
            }
            return;
        }
        if guarded {
            let input = match key {
                Key::Character(s) if s.chars().count() == 1 => {
                    Input::Type(s.chars().next().unwrap())
                }
                Key::Named(NamedKey::Backspace | NamedKey::Delete | NamedKey::Enter) => {
                    Input::OtherTextEdit
                }
                _ => return,
            };
            event.prevent_default();
            let Some([a, b]) = self.absolute_selection() else {
                return;
            };
            let input = if event.is_auto_repeating() {
                Input::Repeat
            } else {
                input
            };
            report(
                self.note
                    .write()
                    .edit_marker(a.min(b)..a.max(b), Mode::Structured, input),
                self.message,
            );
        } else if matches!(field, Field::Title | Field::Heading(_))
            && key == Key::Named(NamedKey::Enter)
        {
            event.prevent_default();
            self.split_heading(field);
        } else if matches!(field, Field::Title | Field::Heading(_) | Field::TitleGap)
            && key == Key::Named(NamedKey::Tab)
            && !event.modifiers().shift()
        {
            event.prevent_default();
            if field == Field::Title {
                self.after_title();
            } else {
                self.vertical(field, true, false);
            }
        }
    }

    pub fn input(mut self, field: Field, value: &str) {
        if self.replace_span_input(field, value) {
            return;
        }
        let md = self.contents.read().clone();
        let parts = self.sections(&md);
        let range = self.range(field, &md);
        let before = &md[range.clone()];
        let proj = self.projection_peek(field);
        let projected_differs = proj.display != before;
        let (removed, inserted) = text_change(&proj.display, value);
        if removed.is_empty() && inserted.is_empty() {
            return;
        }
        let raw = *self.literal.peek();
        let guarded = *self.marker.peek();
        let abs_removed_start = proj.display_to_source(removed.start);
        let abs_removed_end = proj.display_to_source(removed.end);
        if raw.is_none()
            && guarded.is_some()
            && guarded == self.marker_at([abs_removed_start, abs_removed_end])
        {
            // Non-keyboard input (for example an IME commit) may already have
            // reached the native buffer. Restore just that buffer and caret.
            if let Some(root) = self.root.peek().as_ref() {
                let mut doc = root.doc_mut();
                if let Some(id) = doc.get_element_by_id(&field.id()) {
                    doc.with_text_input(id, |mut driver| {
                        driver.select_byte_range(removed.start, removed.start + inserted.len());
                        driver.insert_or_replace_selection(&proj.display[removed.clone()]);
                        driver.select_byte_range(removed.start, removed.end);
                    });
                }
            }
            return;
        }
        if projected_differs && !matches!(field, Field::Title if parts.title.is_none()) {
            let Some((mut splices, mut next_caret)) = proj.map_input(value) else {
                return;
            };
            if let Field::Body(i) = field
                && i > 0
                && !inserted.is_empty()
                && let Some(first) = splices.first_mut()
            {
                let eol = table::eol(&md);
                let blocks = self.blocks.read();
                let previous = blocks.get(i - 1);
                let gap = previous.map_or("", |b| &md[b.line.end..first.range.start]);
                let prefix = if previous.is_some_and(|b| matches!(b.kind, BlockKind::Table(_)))
                    && !gap.contains(|c: char| !c.is_whitespace())
                    && gap.matches('\n').count() < 2
                {
                    eol.repeat(2 - gap.matches('\n').count())
                } else if previous.is_some_and(|b| b.line.end == first.range.start) {
                    eol.to_owned()
                } else {
                    String::new()
                };
                if !prefix.is_empty() {
                    first.text.insert_str(0, &prefix);
                    next_caret += prefix.len();
                }
            }
            let result = self.note.write().splice(&splices);
            if result.is_ok() {
                if let Some((open, close)) = raw {
                    let next = (open < abs_removed_start && abs_removed_end <= close).then(|| {
                        let inserted_len: usize = splices.iter().map(|s| s.text.len()).sum();
                        let deleted_len: usize = splices.iter().map(|s| s.range.len()).sum();
                        (open, close - deleted_len + inserted_len)
                    });
                    self.literal.set(next);
                }
                self.cursor_source.set(Some([next_caret; 2]));
                let next_field = self.field_at(next_caret);
                let next_proj = self.projection(next_field);
                let local = next_proj.source_to_display(next_caret);
                if next_field != field || next_proj.display != value {
                    if next_field != field {
                        self.body.set(None);
                    }
                    self.open(next_field, [next_caret; 2]);
                    return;
                }
                self.note.peek().remember_selection([next_caret; 2]);
                self.track([next_caret; 2]);
                let _ = local;
            }
            report(result, self.message);
            self.refresh_boxes();
            return;
        }
        let absolute = range.start + removed.start..range.start + removed.end;
        if matches!(field, Field::Title)
            && parts.title.is_none()
            && removed.start == 0
            && let Some((hashes, text)) = inserted.split_once(' ')
            && (1..=6).contains(&hashes.len())
            && hashes.bytes().all(|c| c == b'#')
            && !text.is_empty()
        {
            let eol = table::eol(&md);
            let extra = eol.repeat(2);
            let pos = hashes.len() + 1 + text.len();
            report(
                self.note
                    .write()
                    .replace(0..absolute.end, &format!("{hashes} {text}{extra}")),
                self.message,
            );
            self.open(Field::Title, [pos; 2]);
            return;
        }
        let prefix_input = matches!(field, Field::Title | Field::Heading(_))
            && (*self.edit_mode.peek() == super::projection::EditMode::Rich
                || (field == Field::Title && parts.title.is_none()))
            && removed == (0..0)
            && self.title_prefix(field, inserted, false);
        if prefix_input {
            // Non-keyboard input (for example an IME commit) may already have
            // reached the native buffer. Restore just that buffer and caret.
            if let Some(root) = self.root.peek().as_ref() {
                let mut doc = root.doc_mut();
                if let Some(id) = doc.get_element_by_id(&field.id()) {
                    doc.with_text_input(id, |mut driver| {
                        driver.select_byte_range(removed.start, removed.start + inserted.len());
                        driver.insert_or_replace_selection(&before[removed.clone()]);
                        driver.select_byte_range(removed.start, removed.end);
                    });
                }
            }
            return;
        }
        let eol = table::eol(&md);
        let needs_body_separator = field == Field::Body(0)
            && parts.title.is_some()
            && range.start == md.len()
            && !md.ends_with('\n');
        let replacement = match field {
            Field::Title if parts.title.is_none() => format!("# {value}{eol}{eol}"),
            Field::Title | Field::Heading(_)
                if !before.is_empty()
                    && before.bytes().all(|b| b == b'#')
                    && removed == (before.len()..before.len())
                    && !inserted.is_empty()
                    && !inserted.starts_with(['#', ' ', '\t', '\r', '\n']) =>
            {
                format!(" {inserted}")
            }
            Field::Title | Field::Heading(_)
                if before.is_empty() && md[..range.start].ends_with('#') =>
            {
                format!(" {inserted}")
            }
            Field::Body(0) if needs_body_separator => format!("{eol}{eol}{inserted}"),
            Field::Body(i) if !inserted.is_empty() && i > 0 => {
                let blocks = self.blocks.read();
                let previous = blocks.get(i - 1);
                let gap = previous.map_or("", |b| &md[b.line.end..absolute.start]);
                if previous.is_some_and(|b| matches!(b.kind, BlockKind::Table(_)))
                    && !gap.contains(|c: char| !c.is_whitespace())
                    && gap.matches('\n').count() < 2
                {
                    format!("{}{inserted}", eol.repeat(2 - gap.matches('\n').count()))
                } else if previous.is_some_and(|b| b.line.end == absolute.start) {
                    format!("{eol}{inserted}")
                } else {
                    inserted.to_owned()
                }
            }
            _ => inserted.to_owned(),
        };
        // GFM can recognize the delimiter after its first dash. Let the writer
        // finish the whole line, including alignment colons and the final pipe.
        if matches!(field, Field::Body(_)) && !inserted.contains(['\r', '\n']) {
            let caret = removed.start + inserted.len();
            let start = value[..caret].rfind('\n').map_or(0, |n| n + 1);
            let line = value[start..].split('\n').next().unwrap_or("");
            if start > 0
                && line
                    .trim()
                    .bytes()
                    .all(|b| matches!(b, b':' | b'-' | b'|' | b' ' | b'\t'))
            {
                let header = &value[..start - 1];
                let header_start = header.rfind('\n').map_or(0, |n| n + 1);
                if header[header_start..].contains('|') {
                    self.draft_table.set(Some(range.start + header_start));
                }
            }
        }
        let old_count = self.blocks.read().len();
        let result = self.note.write().replace(absolute.clone(), &replacement);
        self.contents.read();
        self.tables.read();
        if result.is_ok()
            && field == Field::Title
            && parts.title.is_none()
            && Sections::read(&self.contents.read()).title.is_some()
        {
            let pos = 2 + value.len();
            self.open(Field::Title, [pos; 2]);
            return;
        }
        if result.is_ok()
            && field == Field::Title
            && parts.title.is_some()
            && Sections::read(&self.contents.read()).title.is_none()
        {
            self.body.set(None);
            let pos = absolute.start + replacement.len();
            self.open(self.field_at(pos), [pos; 2]);
            return;
        }
        if result.is_ok()
            && matches!(field, Field::Heading(_))
            && self.blocks.read().len() != old_count
        {
            let pos = absolute.start + replacement.len();
            self.open(self.field_at(pos), [pos; 2]);
            return;
        }
        if result.is_ok()
            && field == Field::Body(0)
            && parts.title.is_none()
            && Sections::read(&self.contents.read()).title.is_some()
        {
            self.body.set(None);
            let pos = absolute.start + replacement.len();
            self.open(self.field_at(pos), [pos; 2]);
            return;
        }
        if result.is_ok()
            && (field == Field::TitleGap
                || matches!(field, Field::Title | Field::Heading(_)) && inserted != replacement
                || matches!(field, Field::Body(_))
                    && (self.blocks.read().len() != old_count
                        || inserted != replacement && !needs_body_separator))
        {
            let pos = absolute.start + replacement.len();
            self.open(self.field_at(pos), [pos; 2]);
            return;
        }
        if result.is_ok() && needs_body_separator {
            self.body.set(Some(BodyStart::AfterBlankLine));
        }
        if result.is_ok()
            && let Some((open, close)) = raw
        {
            let next = (open < absolute.start && absolute.end <= close)
                .then(|| (open, close - removed.len() + replacement.len()));
            self.literal.set(next);
        }
        report(result, self.message);
        self.track_field(field);
    }
}
