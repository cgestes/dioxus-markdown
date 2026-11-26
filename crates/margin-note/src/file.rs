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

/// A note and the file it is kept in.
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
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("margin-note-file-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("note.sync")
    }

    #[test]
    fn a_note_file_starts_from_the_starter_and_reopens_with_its_edits() {
        let path = scratch("reopen");
        let file = NoteFile::open(&path, "# Start\n").unwrap();
        assert!(!path.exists(), "nothing is written before the first save");
        let note = file.note();
        assert_eq!(note.markdown(), "# Start\n");
        note.replace(8..8, "More.\n").unwrap();
        file.save().unwrap();

        let again = NoteFile::open(&path, "ignored: the file exists").unwrap();
        assert_eq!(again.note().markdown(), "# Start\nMore.\n");
        assert_eq!(again.doc.peer_id(), file.doc.peer_id(), "the same writer continues the history");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_file_written_with_the_old_sync_state_still_opens() {
        let path = scratch("old");
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
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(NoteFile::open(&path, "").unwrap().note().markdown(), "old note");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
