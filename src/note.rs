//! Loro storage for an editable Markdown note.
use crate::checkbox::{Decision, Input, Mode, markdown_markers};
use loro::{
    ExpandType, LoroDoc, LoroResult, LoroValue, StyleConfig, StyleConfigMap, TextDelta, UndoManager,
};
use loro::{UndoItemMeta, cursor::PosType};
use std::{
    ops::Range,
    sync::{Arc, Mutex},
};

const CHECKED: &str = "markdown.checked";

pub use crate::edit::{Splice, text_change};

pub struct Note {
    doc: LoroDoc,
    undo: UndoManager,
    next_selection: Arc<Mutex<UndoItemMeta>>,
    restored_selection: Arc<Mutex<Option<[usize; 2]>>>,
}

impl Note {
    pub fn new(markdown: &str) -> Self {
        let doc = LoroDoc::new();
        doc.get_text("markdown")
            .insert(0, markdown)
            .expect("new text is empty");
        doc.commit();
        Self::from_doc(doc)
    }

    pub fn from_doc(doc: LoroDoc) -> Self {
        let mut styles = StyleConfigMap::new();
        styles.insert(
            CHECKED.into(),
            StyleConfig {
                expand: ExpandType::None,
            },
        );
        doc.config_text_style(styles);
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(0);
        let next_selection = Arc::new(Mutex::new(UndoItemMeta::new()));
        let for_push = Arc::clone(&next_selection);
        undo.set_on_push(Some(Box::new(move |_, _, _| {
            for_push.lock().unwrap().clone()
        })));
        let restored_selection = Arc::new(Mutex::new(None));
        let for_pop = Arc::clone(&restored_selection);
        undo.set_on_pop(Some(Box::new(move |_, _, item| {
            *for_pop.lock().unwrap() = match item.cursors.as_slice() {
                [a, b] => Some([a.pos.pos, b.pos.pos]),
                _ => None,
            };
        })));
        Self {
            doc,
            undo,
            next_selection,
            restored_selection,
        }
    }

    pub fn document(&self) -> &LoroDoc {
        &self.doc
    }

    /// Each structured toggle marks the existing CRDT character. Concurrent
    /// writers can set its value without introducing another character.
    pub fn markdown(&self) -> String {
        let mut markdown = String::new();
        for part in self.doc.get_text("markdown").to_delta() {
            if let TextDelta::Insert { insert, attributes } = part {
                let checked = attributes.as_ref().and_then(|m| m.get(CHECKED));
                if let Some(LoroValue::Bool(checked)) = checked {
                    for c in insert.chars() {
                        markdown.push(match c {
                            ' ' | 'x' | 'X' => {
                                if *checked {
                                    'x'
                                } else {
                                    ' '
                                }
                            }
                            c => c,
                        });
                    }
                } else {
                    markdown.push_str(&insert);
                }
            }
        }
        markdown
    }

    pub fn edit_marker(
        &self,
        selection: Range<usize>,
        mode: Mode,
        input: Input,
    ) -> LoroResult<Decision> {
        if mode == Mode::Literal {
            return Ok(Decision::PassThrough);
        }
        for marker in markdown_markers(&self.markdown()) {
            let action = marker.edit(selection.clone(), mode, input);
            match action {
                Decision::PassThrough => continue,
                Decision::Consume => return Ok(action),
                Decision::SetChecked { at, checked } => {
                    self.doc
                        .get_text("markdown")
                        .mark_utf8(at..at + 1, CHECKED, checked)?;
                    self.doc.commit();
                    return Ok(action);
                }
            }
        }
        Ok(Decision::PassThrough)
    }

    /// Apply an ordinary source edit using UTF-8 byte offsets. A projection
    /// changes only a one-byte ASCII glyph, so offsets are shared by both views.
    pub fn replace(&self, range: Range<usize>, insert: &str) -> LoroResult<()> {
        self.splice(&[Splice::new(range, insert)])
    }

    /// Apply disjoint, source-ordered edits in one undoable Loro commit.
    pub fn splice(&self, edits: &[Splice]) -> LoroResult<()> {
        let before = self.markdown();
        let mut previous = 0;
        for edit in edits {
            if edit.range.start < previous || before.get(edit.range.clone()).is_none() {
                return Err(loro::LoroError::ArgErr(
                    "invalid Markdown byte range".into(),
                ));
            }
            previous = edit.range.end;
        }
        let text = self.doc.get_text("markdown");
        for edit in edits.iter().rev() {
            if before[edit.range.clone()] != edit.text {
                text.delete_utf8(edit.range.start, edit.range.len())?;
                text.insert_utf8(edit.range.start, &edit.text)?;
            }
        }
        self.doc.commit();
        Ok(())
    }

    /// Capture a Loro CRDT cursor pair for a UTF-8 byte range so comment anchors
    /// survive edits elsewhere in the document.
    pub fn anchor_range(
        &self,
        range: Range<usize>,
    ) -> Option<(loro::cursor::Cursor, loro::cursor::Cursor)> {
        let md = self.markdown();
        let start = md.floor_char_boundary(range.start.min(md.len()));
        let end = md.floor_char_boundary(range.end.min(md.len()));
        if start >= end {
            return None;
        }
        let text = self.doc.get_text("markdown");
        let start_u = text.convert_pos(start, PosType::Bytes, PosType::Unicode)?;
        let end_u = text.convert_pos(end, PosType::Bytes, PosType::Unicode)?;
        let a = text.get_cursor(start_u, loro::cursor::Side::Right)?;
        let b = text.get_cursor(end_u, loro::cursor::Side::Left)?;
        Some((a, b))
    }

    /// Resolve a Loro CRDT cursor pair back to current UTF-8 byte offsets.
    pub fn resolve_anchor(
        &self,
        (a, b): &(loro::cursor::Cursor, loro::cursor::Cursor),
    ) -> Option<Range<usize>> {
        let md = self.markdown();
        let text = self.doc.get_text("markdown");
        let ra = self.doc.get_cursor_pos(a).ok()?.current.pos;
        let rb = self.doc.get_cursor_pos(b).ok()?.current.pos;
        let start = text.convert_pos(ra, PosType::Unicode, PosType::Bytes)?;
        let end = text.convert_pos(rb, PosType::Unicode, PosType::Bytes)?;
        let lo = md.floor_char_boundary(start.min(end).min(md.len()));
        let hi = md.floor_char_boundary(start.max(end).min(md.len()));
        (lo < hi).then_some(lo..hi)
    }

    /// Capture both ends before an edit. Loro transforms them as part of its
    /// undo item, including changes made by another author.
    pub fn remember_selection(&self, [anchor, head]: [usize; 2]) {
        let text = self.doc.get_text("markdown");
        let mut item = UndoItemMeta::new();
        for index in [anchor, head] {
            if let Some(pos) = text.convert_pos(index, PosType::Bytes, PosType::Unicode)
                && let Some(cursor) = text.get_cursor(pos, Default::default())
            {
                item.add_cursor(&cursor);
            }
        }
        *self.next_selection.lock().unwrap() = item;
    }

    pub fn take_history_selection(&self) -> Option<[usize; 2]> {
        let [a, b] = self.restored_selection.lock().unwrap().take()?;
        let text = self.doc.get_text("markdown");
        Some([
            text.convert_pos(a, PosType::Unicode, PosType::Bytes)?,
            text.convert_pos(b, PosType::Unicode, PosType::Bytes)?,
        ])
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    pub fn undo(&mut self) -> LoroResult<bool> {
        self.undo.undo()
    }
    pub fn redo(&mut self) -> LoroResult<bool> {
        self.undo.redo()
    }

    pub fn edit_source(&self, after: &str) -> LoroResult<()> {
        let before = self.markdown();
        let (removed, inserted) = text_change(&before, after);
        self.replace(removed, inserted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loro::ExportMode;

    #[test]
    fn disconnected_writers_check_one_character_and_keep_both_unrelated_edits() {
        let a = Note::new("🎧 Go [ ] now\n- [ ] listen\n");
        let b = Note::from_doc(a.document().fork());
        let initial = a.document().oplog_vv();
        let at = a.markdown().find("[ ]").unwrap() + 1;
        a.edit_marker(at..at, Mode::Structured, Input::Type('x'))
            .unwrap();
        b.edit_marker(at + 1..at + 1, Mode::Structured, Input::Type(' '))
            .unwrap();
        a.replace(0..0, "A ").unwrap();
        let end = b.markdown().len();
        b.replace(end..end, "B").unwrap();
        let da = a.document().export(ExportMode::updates(&initial)).unwrap();
        let db = b.document().export(ExportMode::updates(&initial)).unwrap();
        a.document().import(&db).unwrap();
        b.document().import(&da).unwrap();
        b.document().import(&da).unwrap();
        assert_eq!(a.markdown(), "A 🎧 Go [x] now\n- [ ] listen\nB");
        assert_eq!(a.markdown(), b.markdown());
        let saved = a.document().export(ExportMode::Snapshot).unwrap();
        let restored = LoroDoc::new();
        restored.import(&saved).unwrap();
        assert_eq!(Note::from_doc(restored).markdown(), a.markdown());
    }

    #[test]
    fn literal_source_undo_and_export_preserve_the_exact_markdown() {
        let mut note = Note::new("- [ ] listen");
        note.edit_marker(3..3, Mode::Structured, Input::Type(' '))
            .unwrap();
        assert_eq!(note.markdown(), "- [x] listen");
        assert!(note.undo().unwrap());
        assert_eq!(note.markdown(), "- [ ] listen");
        assert!(note.redo().unwrap());
        note.replace(3..4, "value").unwrap();
        assert_eq!(note.markdown(), "- [value] listen");
        note.replace(3..8, " ").unwrap();
        note.replace(2..2, "\\").unwrap();
        assert_eq!(
            note.edit_marker(4..4, Mode::Structured, Input::Type('x'))
                .unwrap(),
            Decision::PassThrough
        );
        assert_eq!(note.markdown(), "- \\[ ] listen");
    }
}
