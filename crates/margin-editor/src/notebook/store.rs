//! Where a notebook's note lives. The editor edits a `Note`; the app that
//! embeds it decides where the note comes from and how it and its comments
//! are kept (an encrypted vault, a sync file, a page on a server, nowhere),
//! and says so in the status bar. What the app's window adds around the
//! notebook is `Host`, not the store.
use crate::comments::CommentThread;
use super::page::PageSource;
use crate::prelude::*;
use margin_note::note::Note;
use std::rc::Rc;

/// A home for the note a notebook edits. Only `use_persistence` is required.
pub trait NoteStore {
    /// The note to open; `None` opens the notebook's initial text.
    fn note(&self) -> Option<Note> {
        None
    }

    /// The comment threads saved with the note; `None` lets the notebook seed
    /// its own (the sample's discussions, or none).
    fn threads(&self) -> Option<Vec<CommentThread>> {
        None
    }

    /// Keep `note` saved, and return the line the status bar shows about it.
    /// The notebook calls this once on every render, as a hook.
    fn use_persistence(&self, note: Signal<Note>) -> Signal<String>;

    /// Save the comment threads, given what was saved last.
    fn save_discussion(&self, _previous: &[u8], _current: &[u8]) -> anyhow::Result<()> {
        Ok(())
    }

    /// Keep an inspector report here; `None` writes it as a file instead.
    /// Returns a description of where it went.
    fn attach_report(&self, _name: &str, _bytes: &[u8]) -> Option<anyhow::Result<String>> {
        None
    }

    /// A page kept on a server that saves when asked: the notebook then
    /// loads and saves the page's comment file, and its bar has Save.
    fn page(&self) -> Option<PageSource> {
        None
    }
}

/// A shared `NoteStore`, as `NotebookOptions` holds it.
#[derive(Clone)]
pub struct Store(Rc<dyn NoteStore>);

impl Store {
    pub fn new(store: impl NoteStore + 'static) -> Self {
        Self(Rc::new(store))
    }
}

impl std::ops::Deref for Store {
    type Target = dyn NoteStore;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl PartialEq for Store {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

/// A note kept nowhere: the status bar says `status` (such as "Not saved").
pub struct InMemory {
    pub status: String,
}

impl NoteStore for InMemory {
    fn use_persistence(&self, _note: Signal<Note>) -> Signal<String> {
        let status = self.status.clone();
        use_signal(move || status)
    }
}
