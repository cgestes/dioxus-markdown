//! A note kept in one local file, unencrypted: `margin --sync-file PATH`.
//! The file is JSON holding the Loro snapshot and the writer it was made as,
//! so edits after a reopen continue the same history.
use crate::note::Note;
use loro::{ExportMode, LoroDoc, PeerID};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What the file holds. Files written before this module also carry sync
/// state that was never used; reading skips it.
#[derive(Serialize, Deserialize)]
struct Saved {
    peer_id: PeerID,
    snapshot: Vec<u8>,
}

/// A note and the file it is kept in. Clones share the note.
#[derive(Clone)]
pub struct NoteFile {
    path: PathBuf,
    doc: LoroDoc,
}

impl NoteFile {
    /// Open the note in `path`. When there is no such file yet the note
    /// starts as `starter`; the file is written on the first `save`.
    pub fn open(path: &Path, starter: &str) -> std::io::Result<Self> {
        let doc = match std::fs::read(path) {
            Ok(bytes) => {
                let saved: Saved = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
                let doc = LoroDoc::new();
                doc.set_peer_id(saved.peer_id).map_err(|e| std::io::Error::other(e.to_string()))?;
                doc.import(&saved.snapshot).map_err(|e| std::io::Error::other(e.to_string()))?;
                doc
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Note::new(starter).document().clone(),
            Err(e) => return Err(e),
        };
        Ok(Self { path: path.to_owned(), doc })
    }

    /// The file's note. Every `Note` this returns edits the same document,
    /// which is what `save` writes.
    pub fn note(&self) -> Note {
        Note::from_doc(self.doc.clone())
    }

    /// Write the note as it is now, replacing the file in one step.
    pub fn save(&self) -> std::io::Result<()> {
        let snapshot = self.doc.export(ExportMode::Snapshot).map_err(|e| std::io::Error::other(e.to_string()))?;
        let bytes = serde_json::to_vec(&Saved { peer_id: self.doc.peer_id(), snapshot }).map_err(std::io::Error::other)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // `<name>.tmp` beside the file: its own, whatever the file is called.
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_file_starts_from_the_starter_and_reopens_with_its_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes").join("note.sync");
        let file = NoteFile::open(&path, "# Start\n").unwrap();
        assert!(!path.exists(), "nothing is written before the first save");
        let note = file.note();
        assert_eq!(note.markdown(), "# Start\n");
        note.replace(8..8, "More.\n").unwrap();
        file.save().unwrap();

        let again = NoteFile::open(&path, "ignored: the file exists").unwrap();
        assert_eq!(again.note().markdown(), "# Start\nMore.\n");
        assert_eq!(again.doc.peer_id(), file.doc.peer_id(), "the same writer continues the history");
    }

    #[test]
    fn a_file_written_with_the_old_sync_state_still_opens() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.sync");
        let note = Note::new("old note");
        let doc = note.document();
        let old = serde_json::json!({
            "epoch": 1,
            "peer_id": doc.peer_id(),
            "snapshot": doc.export(ExportMode::Snapshot).unwrap(),
            "loro_pending": null,
            "pre_root_journal": [],
            "needs_checkpoint": false,
        });
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(NoteFile::open(&path, "").unwrap().note().markdown(), "old note");
    }

    #[test]
    fn files_named_alike_keep_their_own_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        // `a.tmp` would be its own temporary file, and `a.sync` and `a.loro`
        // would share one, if the temporary name replaced the extension.
        for (name, text) in [("a.tmp", "first"), ("a.sync", "second"), ("a.loro", "third")] {
            let file = NoteFile::open(&dir.path().join(name), text).unwrap();
            file.save().unwrap();
        }
        for (name, text) in [("a.tmp", "first"), ("a.sync", "second"), ("a.loro", "third")] {
            assert_eq!(NoteFile::open(&dir.path().join(name), "").unwrap().note().markdown(), text);
        }
        let mut names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        names.sort();
        assert_eq!(names, ["a.loro", "a.sync", "a.tmp"], "no temporary file is left");
    }
}
