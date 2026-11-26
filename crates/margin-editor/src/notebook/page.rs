//! A notebook that edits a page kept on a server: where it saves, whether
//! it has unsaved changes, and the Save control in the editor's bar. The page
//! and its comment file save together: anchors and applied edits in the
//! comment file always describe text the server has. The app supplies the
//! server through `PageFiles` (Osom: the agent's space).
use super::Writing;
use crate::ui::{Button, Variant};
use crate::support::worker::work;
use crate::prelude::*;
pub use margin_editor_core::files::{PageFiles, WriteError};
use std::sync::Arc;

/// The page a notebook edits.
#[derive(Clone)]
pub struct PageSource {
    /// The page's path on the server: `car/maintenance.md`.
    pub path: String,
    pub files: Arc<dyn PageFiles>,
    /// The version the text was read at.
    pub etag: Option<String>,
}

impl PartialEq for PageSource {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.etag == other.etag
    }
}

/// Where the page stands against the server.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageState {
    pub etag: Option<String>,
    /// The document revision last saved (or read).
    pub saved_rev: u64,
    pub saving: bool,
    /// The last save's outcome: "Saved", or why it failed.
    pub note: String,
}

impl Writing {
    /// Changes not on the server yet.
    pub fn page_dirty(&self) -> bool {
        self.page.read().is_some() && *self.doc_rev.read() != self.page_state.read().saved_rev
    }

    /// Write the page if nobody changed it since it was read; then refresh
    /// the comment anchors from the saved text and save the comment file.
    pub fn save_page(self) {
        let Some(page) = self.page.peek().clone() else {
            return;
        };
        let mut state = self.page_state;
        if state.peek().saving {
            return;
        }
        let rev = *self.doc_rev.peek();
        let text = self.contents.peek().clone();
        let etag = state.peek().etag.clone();
        {
            let mut s = state.write();
            s.saving = true;
            s.note = "Saving…".into();
        }
        spawn(async move {
            let (files, path) = (page.files.clone(), page.path.clone());
            let result = work(move || files.write(&path, &text, etag.as_deref())).await;
            let saved = result.is_ok();
            {
                let mut s = state.write();
                s.saving = false;
                match result {
                    Ok(etag) => {
                        s.etag = etag;
                        s.saved_rev = rev;
                        s.note = "Saved".into();
                    }
                    Err(e) => s.note = e.to_string(),
                }
            }
            // Anchors describe the saved text: refresh them only if nothing
            // changed while the save was on its way.
            if saved && *self.doc_rev.peek() == rev {
                self.commit_comments();
            }
        });
    }
}

/// The page's name, state and Save button, in the editor's bar.
#[component]
pub(crate) fn PageBar(writing: Writing) -> Element {
    let page = writing.page.read();
    let Some(src) = page.as_ref() else {
        return rsx! {};
    };
    let dirty = writing.page_dirty();
    let state = writing.page_state.read();
    let note = if state.saving {
        "Saving…".to_string()
    } else if dirty {
        "Edited".to_string()
    } else {
        state.note.clone()
    };
    rsx! {
        div { class: "flex items-center gap-2 ml-2 pl-2 border-0 border-l border-solid border-line [&_.ui-btn]:h-6",
            span { class: "text-muted type-mono-ui [.notebook.embedded_&]:hidden", "{src.path}" }
            if !note.is_empty() { span { class: "text-faint type-hint", "{note}" } }
            Button { id: "save-page", label: "Save", variant: Variant::Primary, disabled: !dirty || state.saving,
                onclick: move |_| writing.save_page() }
        }
    }
}
