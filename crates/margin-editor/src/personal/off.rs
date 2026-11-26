//! Built without the `vault` feature: there is no personal session, and
//! `margin` opens the Field notes sample in its place.
use crate::comments::CommentThread;
use anyhow::Result;
use crate::prelude::*;
use margin_note::note::Note;

/// What the status bar says when a note is kept nowhere: here, always.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const UNSAVED: &str = "Not saved: this build has no vault (build with --features vault)";
/// A browser has no vault to build with yet.
#[cfg(target_arch = "wasm32")]
pub(crate) const UNSAVED: &str = "Not saved: the note lasts until this page closes";

/// The notes menu's open state; only a vault session shows the menu.
#[derive(Clone)]
pub(crate) struct Menu(pub Signal<bool>);

/// A personal vault session. Without the vault there is none: the type has
/// no values, so every path that needs one compiles away.
#[derive(Clone)]
pub(crate) enum Session {}

impl Session {
    pub(crate) fn note(&self) -> Note {
        match *self {}
    }

    pub(crate) fn threads(&self) -> Vec<CommentThread> {
        match *self {}
    }

    pub(crate) fn save_discussion(&self, _previous: &[u8], _current: &[u8]) -> Result<()> {
        match *self {}
    }

    pub(crate) fn attach_report(&self, _name: &str, _bytes: &[u8]) -> Result<String> {
        match *self {}
    }
}

/// Nothing to persist without a session.
pub(crate) fn use_persistence(_session: Option<Session>, _note: Signal<Note>) -> Signal<String> {
    use_signal(String::new)
}

/// `margin` without the vault: the Field notes sample, in memory.
#[component]
pub(crate) fn Desktop(split: bool) -> Element {
    rsx! { crate::SampleEditor { split, inspect: false } }
}
