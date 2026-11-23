//! Commands that can change more than the focused native input.
use super::{Field, Writing, report, table};
use dioxus_native::prelude::*;
use margin_editor_core::note::text_change;

impl Writing {
    fn edit_range(mut self, range: std::ops::Range<usize>, text: &str) {
        if let Some(absolute) = self.absolute_selection() {
            self.note.peek().remember_selection(absolute);
        }
        let end = range.start + text.len();
        let result = self.note.write().replace(range, text);
        if result.is_ok() {
            self.body.set(None);
            self.open(self.field_at(end), [end; 2]);
        }
        report(result, self.message);
    }

    pub(super) fn after_title(self) {
        let md = self.contents.read();
        let start = super::Sections::read(&md).body;
        let target = if start == md.len() {
            Field::Body(0)
        } else {
            self.field_at(start)
        };
        self.open(target, [start; 2]);
    }

    pub(super) fn split_heading(self, field: Field) {
        let md = self.contents.read().clone();
        let Some([a, b]) = self.absolute_selection() else {
            return;
        };
        if field == Field::Title && super::Sections::read(&md).title.is_none() {
            self.after_title();
            return;
        }
        let eol = table::eol(&md);
        let range = self.range(field, &md);
        // Keep any closing ATX hashes on the original heading.
        let line_end = range.end
            + md[range.end..]
                .find(['\r', '\n'])
                .unwrap_or(md.len() - range.end);
        if a == b && a == range.end {
            self.edit_range(line_end..line_end, eol);
        } else if line_end > range.end {
            let start = a.min(b);
            let end = a.max(b);
            let after = format!("{}{eol}{}", &md[range.end..line_end], &md[end..range.end]);
            let caret = start + line_end - range.end + eol.len();
            let mut this = self;
            let result = this.note.write().replace(start..line_end, &after);
            if result.is_ok() {
                this.open(this.field_at(caret), [caret; 2]);
            }
            report(result, this.message);
        } else {
            self.edit_range(a.min(b)..a.max(b), eol);
        }
    }

    pub(super) fn replace_span_input(self, field: Field, value: &str) -> bool {
        let Some([a, b]) = *self.span.peek() else {
            return false;
        };
        let proj = self.projection(field);
        let local = proj.source_to_display(b);
        let before = proj.display.as_str();
        let inserted = value
            .strip_prefix(&before[..local])
            .and_then(|rest| rest.strip_suffix(&before[local..]))
            .unwrap_or_else(|| text_change(before, value).1);
        self.edit_range(a.min(b)..a.max(b), inserted);
        true
    }

    fn join_boundary(self, field: Field, head: usize, forward: bool, md: &str) -> bool {
        let bounds = self.range(field, md);
        if head != if forward { bounds.end } else { bounds.start }
            || !forward && matches!(field, Field::Title | Field::Heading(_))
        {
            return false;
        }
        let Some(target) = self.neighbor(field, forward) else {
            return false;
        };
        // A cell boundary is an edit stop. Joining through it would
        // remove pipes or turn an ordinary paragraph into a table row.
        if matches!(target, Field::Cell(..)) {
            return false;
        }
        let adjacent = self.range(target, md);
        let range = if forward {
            head..adjacent.start
        } else {
            adjacent.end..head
        };
        if range.is_empty() || !md[range.clone()].contains('\n') {
            return false;
        }
        self.edit_range(range, "");
        true
    }

    pub(super) fn document_key(self, field: Field, e: &KeyboardEvent) -> bool {
        let mods = e.modifiers();
        let primary = mods.intersects(Modifiers::META | Modifiers::CONTROL) && !mods.alt();
        let key = e.key();
        if primary && key == Key::Named(NamedKey::Enter) && !mods.shift() {
            if self.toggle_line_task(field) {
                e.prevent_default();
                e.stop_propagation();
                return true;
            }
        }
        if primary && let Key::Character(k) = &key {
            use margin_editor_core::format::Style;
            let style = match (k.to_ascii_lowercase().as_str(), mods.shift()) {
                ("b", false) => Some(Style::Bold),
                ("i", false) => Some(Style::Italic),
                ("x", true) => Some(Style::Strike),
                ("e", false) => Some(Style::Code),
                ("k", false) => Some(Style::Link),
                _ => None,
            };
            if let Some(style) = style
                && self.format(style)
            {
                e.prevent_default();
                e.stop_propagation();
                return true;
            }
            if !mods.shift()
                && k.len() == 1
                && let Some(level @ 0..=6) = k.bytes().next().and_then(|b| b.checked_sub(b'0'))
                && self.set_heading_level(field, level as usize)
            {
                e.prevent_default();
                e.stop_propagation();
                return true;
            }
        }
        let span = self.span.peek().is_some();
        if !span
            && !primary
            && !matches!(
                key,
                Key::Named(
                    NamedKey::ArrowLeft
                        | NamedKey::ArrowRight
                        | NamedKey::Delete
                        | NamedKey::Backspace
                )
            )
        {
            return false;
        }
        let md = self.contents.read().clone();
        let Some([anchor, head]) = self.absolute_selection() else {
            return false;
        };
        let selected = anchor.min(head)..anchor.max(head);
        let note_edge = mods.meta()
            && matches!(key, Key::Named(NamedKey::ArrowUp | NamedKey::ArrowDown))
            || primary
                && !matches!(field, Field::Cell(..))
                && matches!(key, Key::Named(NamedKey::Home | NamedKey::End));
        if note_edge {
            let end = matches!(key, Key::Named(NamedKey::ArrowDown | NamedKey::End));
            let pos = if end {
                md.len()
            } else if mods.shift() {
                0
            } else {
                self.range(self.field_at(0), &md).start
            };
            self.open(
                self.field_at(pos),
                [if mods.shift() { anchor } else { pos }, pos],
            );
        } else if primary && matches!(&key,Key::Character(s) if s.eq_ignore_ascii_case("a")) {
            self.open(self.field_at(md.len()), [0, md.len()]);
        } else if !span
            && primary
            && matches!(field, Field::Title | Field::Heading(_) | Field::TitleGap)
            && matches!(&key,Key::Character(k) if k.eq_ignore_ascii_case("v"))
        {
            // Single-line platform inputs discard line endings. Paste a
            // Markdown fragment at the document selection before that happens.
            let Some(text) = self
                .root
                .peek()
                .as_ref()
                .and_then(|h| h.doc().shell_provider.get_clipboard_text().ok())
            else {
                return false;
            };
            if !text.contains(['\r', '\n']) {
                return false;
            }
            self.edit_range(selected, &text);
        } else if span
            && primary
            && matches!(&key,Key::Character(s) if s.eq_ignore_ascii_case("c") || s.eq_ignore_ascii_case("x") || s.eq_ignore_ascii_case("v"))
        {
            let Some(handle) = self.root.peek().clone() else {
                return false;
            };
            let shell = handle.doc().shell_provider.clone();
            if matches!(&key,Key::Character(s) if s.eq_ignore_ascii_case("v")) {
                if let Ok(s) = shell.get_clipboard_text() {
                    self.edit_range(selected, &s);
                }
            } else if shell
                .set_clipboard_text(md[selected.clone()].to_owned())
                .is_ok()
                && matches!(&key, Key::Character(s) if s.eq_ignore_ascii_case("x"))
            {
                self.edit_range(selected, "");
            }
        } else if (mods - Modifiers::SHIFT).is_empty() {
            match key {
                Key::Named(NamedKey::Backspace | NamedKey::Delete) if span => {
                    self.edit_range(selected, "")
                }
                Key::Named(NamedKey::Enter) if span => self.edit_range(selected, table::eol(&md)),
                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) => {
                    if !self.horizontal(
                        field,
                        [anchor, head],
                        key == Key::Named(NamedKey::ArrowRight),
                        mods.shift(),
                    ) {
                        return false;
                    }
                }
                Key::Named(NamedKey::Delete | NamedKey::Backspace)
                    if selected.is_empty()
                        && !matches!(field, Field::Cell(..) | Field::TitleGap) =>
                {
                    let forward = key == Key::Named(NamedKey::Delete);
                    if !self.join_boundary(field, head, forward, &md) {
                        return false;
                    }
                }
                _ => return false,
            }
        } else {
            return false;
        }
        e.prevent_default();
        e.stop_propagation();
        true
    }
}
