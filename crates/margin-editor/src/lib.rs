//! The Markdown editor: the notebook (Rich, Live and Source views, tables,
//! comments, undo) that an app opens in a window or a page, or embeds, as Osom
//! does. Where a note lives is up to the app (`NoteStore`).
mod notebook;
mod comments;
mod platform;
mod reading;
pub mod support;
// Always built on native: a feature would make test builds compile the editor twice.
#[cfg(not(target_arch = "wasm32"))]
pub mod testing;
#[cfg(test)]
mod tests;

pub(crate) use margin_ui::prelude;
use platform::NodeHandle;
use prelude::*;
use margin_editor_core::table;
use margin_ui as ui;
#[cfg(test)]
use margin_ui::typefaces;
use notebook::*;

// What an app builds a notebook from.
pub use comments::{CommentAgent, CommentThread, seed_sample_comments};
pub use notebook::{Comments, Host, HostAction, HostBar, InMemory, NoteStore, NotebookOptions, Store, View, notebook_view};
// The document a notebook edits: a `NoteStore` hands one over.
pub use margin_note::note::Note;
// Markdown to read.
pub use reading::MarkdownView;
pub(crate) use reading::callout_icon;
pub use notebook::page::{PageFiles, PageSource};
use ui::Icon;

/// *Field notes*, the sample note: the demo, a new vault's first note, the tests.
pub const SAMPLE: &str = "# Field notes\n\nRecorder packed [ ]. Rain cover ready [x].\n\n## Listening stops\n\n| Stop | Listen for | Take |\n| :--- | :--- | ---: |\n| North lock | Water through the gate | 45 s |\n| Footbridge | Steps on the steel deck | 30 s |\n\n- [ ] Leave **ten seconds** of silence.\n- [x] Set the recorder to `48 kHz`.";

/// The note an app's input-check window opens (`--test-window`, `?test-window`).
pub const INPUT_CHECK: &str = "# Input check\n\nabcd\nefgh\nijkl\nmnop\n\n| A | B |\n| --- | --- |\n| one | two |\n| three | four |";

/// The editor's own styles; tokens and components come from `ui::CSS`.
const CSS: &str = concat!(
    include_str!("notebook/editor.css"),
    "\n",
    include_str!("notebook/inspector.css"),
);

/// The app's name, as `launch` was given it: the notebook's window titles
/// start with it.
static APP_TITLE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub(crate) fn app_title() -> &'static str {
    APP_TITLE.get().map_or("Margin", String::as_str)
}

/// Open a window (or, in a browser, the page) titled `title` whose content is `root`.
pub fn launch(title: &str, size: (f64, f64), root: fn() -> Element) {
    let _ = APP_TITLE.set(title.to_owned());
    platform::launch(title, size, root);
}

#[cfg(test)]
fn notebook(initial: &'static str) -> Element {
    notebook_view(initial, NotebookOptions::default())
}
