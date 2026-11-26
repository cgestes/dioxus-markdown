//! `margin`: the notebook app. It opens the Markdown editor in a window on
//! your personal notebook (the encrypted vault, with the `vault` feature), a
//! standalone sync file, or the Field notes sample, and shows the design
//! system's gallery (`margin ui`).
mod cli;
mod personal;
#[cfg(test)]
mod tests;

use margin_ui::prelude::*;
use margin_editor::{Comments, INPUT_CHECK, InMemory, MarkdownView, NoteStore, NotebookOptions, SAMPLE, Store, notebook_view};
use margin_note::note::Note;
use margin_ui::gallery::{GRID, Gallery, section};
use margin_ui::{Card, CardTitle, Icon};

fn main() {
    margin_runtime::restart::answer_build_probe(if cfg!(feature = "vault") { "margin, with the vault" } else { "margin, without the vault" });
    let args = cli::init();
    let size = if args.is_gallery() { (1180.0, 900.0) } else { (920.0, 650.0) };
    margin_runtime::launch("Margin", size, app);
}

fn app() -> Element {
    let args = cli::current();
    if args.is_gallery() {
        return gallery();
    }
    if args.test_window {
        return notebook_view(INPUT_CHECK, NotebookOptions { inspect: true, ..Default::default() });
    }
    let inspect = args.test_editor;
    if let Some(path) = args.sync_file.clone() {
        return rsx! { SyncFileEditor { path, inspect } };
    }
    if inspect || args.demo {
        return rsx! { SampleEditor { inspect } };
    }
    rsx! { personal::Desktop {} }
}

/// `margin ui`: the design system's gallery, then Markdown as the editor
/// reads it. The design system has its styles and knows no Markdown.
fn gallery() -> Element {
    rsx! {
        Gallery {
            {section("Markdown", rsx! {
                div { class: GRID,
                    Card { CardTitle { text: "page", icon: Icon::File } MarkdownView { text: SAMPLE } }
                    Card { CardTitle { text: "compact", icon: Icon::Chat } MarkdownView { text: SAMPLE, compact: true } }
                }
            })}
        }
    }
}

/// The sample note's options: sample comment threads unless inspecting.
fn sample_options(inspect: bool) -> NotebookOptions {
    NotebookOptions {
        inspect,
        comments: if inspect { Comments::Empty } else { Comments::Sample },
        ..Default::default()
    }
}

/// The sample in memory: the status bar says it is not saved.
#[component]
fn SampleEditor(inspect: bool) -> Element {
    let store = use_hook(|| Store::new(InMemory { status: personal::UNSAVED.to_owned() }));
    notebook_view(SAMPLE, NotebookOptions { store: Some(store), ..sample_options(inspect) })
}

/// A standalone Loro history file as the notebook's store: every change to
/// the note is written back to the file.
struct SyncFileStore {
    path: std::path::PathBuf,
    state: std::rc::Rc<margin_note::sync::NoteSync>,
}

impl NoteStore for SyncFileStore {
    fn note(&self) -> Option<Note> {
        Some(Note::from_doc(self.state.note().document().clone()))
    }

    fn use_persistence(&self, note: Signal<Note>) -> Signal<String> {
        let mut status = use_signal(String::new);
        let (path, state) = (self.path.clone(), self.state.clone());
        use_effect(move || {
            note.read();
            match state.save_to_file(&path) {
                Ok(()) if !status.peek().is_empty() => status.set(String::new()),
                Ok(()) => {}
                Err(error) => status.set(format!("Couldn’t save the sync file. {error}")),
            }
        });
        status
    }
}

#[component]
fn SyncFileEditor(path: std::path::PathBuf, inspect: bool) -> Element {
    use margin_note::sync::NoteSync;
    let loaded = use_hook(|| {
        let sync = match NoteSync::load_from_file(&path) {
            Ok(sync) => sync,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => NoteSync::new(Note::new(SAMPLE), 1),
            Err(e) => return Err(e.to_string()),
        };
        Ok(Store::new(SyncFileStore { path, state: std::rc::Rc::new(sync) }))
    });
    let store = match loaded {
        Ok(store) => store,
        Err(error) => return rsx! { p { role: "alert", "Couldn’t open the sync file. {error}" } },
    };
    notebook_view(SAMPLE, NotebookOptions { store: Some(store), ..sample_options(inspect) })
}
