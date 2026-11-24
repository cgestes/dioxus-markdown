//! One native text input owns the entire Markdown document. The facing page
//! maps its shaped glyphs back to that input and never takes over typing.
use super::{ByteSelection, EditorButton, Field, Writing, lists, mounted_field, report, selection};
#[path = "reading.rs"]
mod reading;
use dioxus_native::prelude::*;
use margin_editor_core::{
    checkbox::{Input, Mode},
    edit::text_change,
    table,
};
use reading::Reading;

#[derive(Clone, Copy, PartialEq)]
struct Source {
    writing: Writing,
    caret: Signal<ByteSelection>,
    // Virtual cells in a short Markdown row share one physical source position.
    missing: Signal<Option<(usize, usize, usize)>>,
}

impl Source {
    fn track(mut self, range: ByteSelection) {
        if *self.caret.peek() != range {
            self.caret.set(range);
            self.missing.set(None);
        }
        self.writing.track(range);
        self.writing.note.peek().remember_selection(range);
    }

    fn read(self) {
        if let Some(range) = selection(self.writing.root, Field::Source) {
            self.track(range);
        }
    }

    fn place(mut self, range: ByteSelection) {
        self.track(range);
        self.writing.focus.set(Some((Field::Source, range)));
    }

    fn cell(mut self, start: usize, row: usize, col: usize, range: Option<ByteSelection>) {
        // A structural edit can change all of the source coordinates in a table.
        let tables = self.writing.tables.read();
        let Some(t) = tables.iter().find(|t| t.range.start == start) else {
            return;
        };
        let target = t.cell(row, col);
        self.place(range.unwrap_or([target.start, target.end]));
        self.missing
            .set((col >= t.rows[row].cells.len()).then_some((start, row, col)));
    }

    // Loro commits synchronously. Dioxus may still hold the previous generation
    // of its derived table memo until the next render.
    fn after_table_edit(mut self, start: usize, row: usize, col: usize, at_end: bool) {
        let markdown = self.writing.note.peek().markdown();
        if let Some(table) = table::tables(&markdown)
            .into_iter()
            .find(|t| t.range.start == start)
            && row < table.rows.len()
            && col < table.width()
        {
            let cell = table.cell(row, col);
            self.place([if at_end { cell.end } else { cell.start }, cell.end]);
            self.missing
                .set((col >= table.rows[row].cells.len()).then_some((start, row, col)));
        }
    }

    fn grow(mut self, start: usize, column: bool, col: usize) {
        let tables = self.writing.tables.peek();
        let Some(t) = tables.iter().find(|t| t.range.start == start) else {
            return;
        };
        let (changes, row, col) = if column {
            (t.append_column(), 0, t.width())
        } else {
            (
                vec![t.append_row(&self.writing.contents.peek())],
                t.rows.len(),
                col,
            )
        };
        drop(tables);
        self.read();
        report(
            self.writing.note.write().splice(&changes),
            self.writing.message,
        );
        self.after_table_edit(start, row, col, false);
    }

    fn selected_cell(self, grid: &table::Table, range: ByteSelection) -> Option<(usize, usize)> {
        let [a, b] = range;
        if a.min(b) < grid.range.start || a.max(b) > grid.range.end {
            return None;
        }
        let (r, c) = self
            .missing
            .read()
            .filter(|(at, r, c)| {
                *at == grid.range.start && *r < grid.rows.len() && *c < grid.width()
            })
            .map_or_else(|| grid.position(b), |(_, r, c)| (r, c));
        let cell = grid.cell(r, c);
        (a.min(b) >= cell.start && a.max(b) <= cell.end).then_some((r, c))
    }

    fn delete_column(mut self, start: usize) {
        let Some(range) = selection(self.writing.root, Field::Source) else {
            return;
        };
        let tables = self.writing.tables.peek();
        let Some(grid) = tables.iter().find(|t| t.range.start == start) else {
            return;
        };
        let Some((row, col)) = self.selected_cell(grid, range) else {
            return;
        };
        let edits = grid.delete_column(&self.writing.contents.peek(), col);
        if edits.is_empty() {
            return;
        }
        let survivor = col.min(grid.width() - 2);
        drop(tables);
        self.track(range);
        let result = self.writing.note.write().splice(&edits);
        if result.is_ok() {
            self.after_table_edit(start, row, survivor, false);
        }
        report(result, self.writing.message);
    }

    fn marker(self, range: ByteSelection) -> bool {
        self.writing.literal.peek().is_none() && self.writing.marker_at(range).is_some()
    }

    fn key(mut self, event: KeyboardEvent, split: bool) {
        let Some(range @ [a, b]) = selection(self.writing.root, Field::Source) else {
            return;
        };
        self.track(range);
        if event.is_composing() {
            return;
        }
        let key = event.key();
        let mods = event.modifiers();
        let modified = mods.intersects(Modifiers::CONTROL | Modifiers::META);
        if !split && modified && !mods.alt() {
            use margin_editor_core::format::{InlineSelection, Style};
            if let Key::Character(k) = &key {
                let style = match (k.to_ascii_lowercase().as_str(), mods.shift()) {
                    ("b", false) => Some(Style::Bold),
                    ("i", false) => Some(Style::Italic),
                    ("x", true) => Some(Style::Strike),
                    ("e", false) => Some(Style::Code),
                    ("k", false) => Some(Style::Link),
                    _ => None,
                };
                if let Some(style) = style {
                    let md = self.writing.contents.peek().clone();
                    if let Some(sel) = InlineSelection::read(&md, range) {
                        event.prevent_default();
                        let edit = sel.toggle(&md, style);
                        self.writing.note.peek().remember_selection(range);
                        let result = self.writing.note.write().splice(&edit.changes);
                        if result.is_ok() {
                            self.place(edit.selection);
                        }
                        report(result, self.writing.message);
                        return;
                    }
                }
            }
        }
        if !split {
            if !modified && !mods.alt() {
                let command = match key {
                    Key::Named(NamedKey::Enter) if !mods.shift() => Some(lists::Command::Continue),
                    Key::Named(NamedKey::Backspace) if !mods.shift() => {
                        Some(lists::Command::Backspace)
                    }
                    Key::Named(NamedKey::Tab) => Some(if mods.shift() {
                        lists::Command::Outdent
                    } else {
                        lists::Command::Indent
                    }),
                    _ => None,
                };
                if let Some(edit) =
                    command.and_then(|cmd| lists::edit(&self.writing.contents.peek(), range, cmd))
                {
                    event.prevent_default();
                    report(
                        self.writing.note.write().splice(&edit.changes),
                        self.writing.message,
                    );
                    self.place(edit.caret);
                }
            }
            return;
        }
        let lo = a.min(b);
        let hi = a.max(b);
        let guarded = self.marker(range);
        if key == Key::Named(NamedKey::F2)
            && let Some(open) = self.writing.marker_at(range)
        {
            event.prevent_default();
            if !event.is_auto_repeating() {
                let next = self
                    .writing
                    .literal
                    .peek()
                    .is_none()
                    .then_some((open, open + 2));
                self.writing.literal.set(next);
            }
            return;
        }
        if key == Key::Named(NamedKey::Escape) {
            self.writing.literal.set(None);
            return;
        }
        if mods.alt() {
            return;
        }
        let modified = mods.intersects(Modifiers::CONTROL | Modifiers::META);
        if guarded {
            let input = match &key {
                Key::Character(k) if !modified => k.chars().next().map(Input::Type),
                Key::Character(k)
                    if modified && (k.eq_ignore_ascii_case("v") || k.eq_ignore_ascii_case("x")) =>
                {
                    Some(Input::OtherTextEdit)
                }
                Key::Named(NamedKey::Enter | NamedKey::Delete | NamedKey::Backspace)
                    if !modified =>
                {
                    Some(Input::OtherTextEdit)
                }
                _ => None,
            };
            if let Some(input) = input {
                event.prevent_default();
                report(
                    self.writing.note.write().edit_marker(
                        lo..hi,
                        Mode::Structured,
                        if event.is_auto_repeating() {
                            Input::Repeat
                        } else {
                            input
                        },
                    ),
                    self.writing.message,
                );
                self.place(range);
                return;
            }
        }
        let paste = modified && matches!(&key,Key::Character(k) if k.eq_ignore_ascii_case("v"));
        if modified && !paste {
            return;
        }
        if paste || matches!(key, Key::Named(NamedKey::Tab | NamedKey::Enter)) {
            let tables = self.writing.tables.peek();
            if let Some(t) = tables
                .iter()
                .find(|t| t.range.start <= lo && hi <= t.range.end)
            {
                let start = t.range.start;
                if let Some((r, c)) = self.selected_cell(t, range) {
                    if paste {
                        let clipboard = self
                            .writing
                            .root
                            .peek()
                            .as_ref()
                            .and_then(|h| h.doc().shell_provider.get_clipboard_text().ok());
                        if let Some(text) = clipboard
                            && text.contains(['\n', '\r', '\t'])
                        {
                            event.prevent_default();
                            let changes = t.paste(&self.writing.contents.peek(), r, c, &text);
                            drop(tables);
                            report(
                                self.writing.note.write().splice(&changes),
                                self.writing.message,
                            );
                            self.after_table_edit(start, r, c, false);
                        }
                        return;
                    }
                    let w = t.width();
                    let height = t.rows.len();
                    let step = if key == Key::Named(NamedKey::Tab) {
                        1
                    } else {
                        w
                    };
                    let index = r * w + c;
                    let dest = if mods.shift() {
                        index.checked_sub(step)
                    } else {
                        Some(index + step)
                    };
                    drop(tables);
                    event.prevent_default();
                    if let Some(n) = dest {
                        if n / w >= height {
                            self.grow(start, false, n % w)
                        } else {
                            self.cell(start, n / w, n % w, None)
                        }
                    } else {
                        self.place([start; 2]);
                    }
                    return;
                }
            }
        }
        if modified {
            return;
        }
        let command = match key {
            Key::Named(NamedKey::Enter) if !mods.shift() => lists::Command::Continue,
            Key::Named(NamedKey::Backspace) if !mods.shift() => lists::Command::Backspace,
            Key::Named(NamedKey::Tab) => {
                if mods.shift() {
                    lists::Command::Outdent
                } else {
                    lists::Command::Indent
                }
            }
            _ => return,
        };
        let edit = lists::edit(&self.writing.contents.peek(), range, command);
        if let Some(edit) = edit {
            event.prevent_default();
            report(
                self.writing.note.write().splice(&edit.changes),
                self.writing.message,
            );
            self.place(edit.caret);
        }
    }

    fn input(mut self, value: &str, split: bool) {
        let before = self.writing.contents.peek();
        let (removed, inserted) = text_change(&before, value);
        if removed.is_empty() && inserted.is_empty() {
            return;
        }
        let range = *self.caret.peek();
        if split
            && self.marker(range)
            && self.writing.marker_at([removed.start, removed.end]) == self.writing.marker_at(range)
        {
            // IME commits and menu paste can arrive without a printable keydown.
            if let Some(h) = self.writing.root.peek().as_ref() {
                let mut doc = h.doc_mut();
                if let Some(id) = doc.get_element_by_id("markdown") {
                    doc.with_text_input(id, |mut driver| {
                        driver.select_byte_range(removed.start, removed.start + inserted.len());
                        driver.insert_or_replace_selection(&before[removed.clone()]);
                        driver.select_byte_range(range[0], range[1]);
                    });
                }
            }
            let mut chars = inserted.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                drop(before);
                report(
                    self.writing.note.write().edit_marker(
                        range[0].min(range[1])..range[0].max(range[1]),
                        Mode::Structured,
                        Input::Type(c),
                    ),
                    self.writing.message,
                );
                self.place(range);
            }
            return;
        }
        let missing = *self.missing.peek();
        if split
            && let Some((start, r, c)) = missing
            && removed.is_empty()
            && !inserted.contains(['\r', '\n'])
        {
            let tables = self.writing.tables.peek();
            if let Some(t) = tables
                .iter()
                .find(|t| t.range.start == start && r < t.rows.len() && c < t.width())
                && t.cell(r, c).start == removed.start
                && c >= t.rows[r].cells.len()
            {
                let edit = t.edit_cell(&before, r, c, inserted);
                drop(tables);
                drop(before);
                report(
                    self.writing.note.write().splice(&[edit]),
                    self.writing.message,
                );
                self.after_table_edit(start, r, c, true);
                return;
            }
        }
        let literal = *self.writing.literal.peek();
        if let Some((open, close)) = literal {
            self.writing.literal.set(
                (open < removed.start && removed.end <= close)
                    .then(|| (open, close - removed.len() + inserted.len())),
            );
        }
        drop(before);
        report(
            self.writing.note.write().replace(removed, inserted),
            self.writing.message,
        );
        self.read();
    }
}

#[component]
pub(super) fn DocumentEditor(writing: Writing, split: bool) -> Element {
    let source = Source {
        writing,
        caret: use_signal(|| [0; 2]),
        missing: use_signal(|| None),
    };
    let md = writing.contents.read();
    use_effect(move || {
        writing.focus.read();
        if writing.focus.peek().is_none() {
            source.read();
        }
    });
    rsx! {
        div { class: if split {"split-layout"} else {"source-layout"},
            div { class:"split-source",
                if split { div {class:"pane-label", "Markdown"} }
                textarea { id:"markdown", aria_label:"Markdown source", value:md.as_str(),
                    onmounted:move|e|mounted_field(e,Field::Source,writing.focus),
                    onpointerup:move|_|source.read(), onkeyup:move|_|source.read(),
                    onkeydown:move|e|source.key(e,split), oninput:move|e|source.input(&e.value(),split),
                    "{md}"
                }
            }
            if split { Reading { source } }
        }
        if split && writing.marker.read().is_some() {
            footer { if writing.literal.read().is_some() { "Editing bracket text · F2 to finish" } else { "Space or X toggles · F2 edits the brackets" } }
        }
    }
}
