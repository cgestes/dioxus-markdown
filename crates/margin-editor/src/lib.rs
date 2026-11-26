//! Margin: the notebook editor, its comments and personal notes.
mod cli;
mod notebook;
mod comments;
mod personal;
mod platform;
pub mod support;
#[cfg(test)]
mod tests;

use dioxus_native::{NodeHandle, prelude::*};
use margin_note::note::Note;
use margin_editor_core::table;
use margin_ui as ui;
use margin_ui::typefaces;
use notebook::*;

// What a host (the Topics window, Osom) builds a notebook from.
pub use comments::CommentAgent;
pub use notebook::{Comments, HostAction, HostBar, NotebookOptions, View, notebook_view};
use ui::Icon;

#[cfg(target_os = "macos")]
use platform::macos;

const SAMPLE: &str = "# Field notes\n\nRecorder packed [ ]. Rain cover ready [x].\n\n## Listening stops\n\n| Stop | Listen for | Take |\n| :--- | :--- | ---: |\n| North lock | Water through the gate | 45 s |\n| Footbridge | Steps on the steel deck | 30 s |\n\n- [ ] Leave **ten seconds** of silence.\n- [x] Set the recorder to `48 kHz`.";
/// The editor's own styles; tokens and components come from `ui::CSS`.
const CSS: &str = concat!(
    include_str!("notebook/editor.css"),
    "\n",
    include_str!("notebook/split.css"),
    "\n",
    include_str!("notebook/inspector.css"),
    "\n",
    include_str!("comments/comments.css"),
);

/// The `margin` app: the notebook window.
pub fn run() {
    support::restart::answer_build_probe();
    let args = cli::init();
    let size = if args.is_gallery() {
        (1180.0, 900.0)
    } else {
        (920.0, 650.0)
    };
    launch("Margin", size, app);
}

/// Open a window titled `title` whose content is `root`.
pub fn launch(title: &str, size: (f64, f64), root: fn() -> Element) {
    let window = dioxus_native::WindowAttributes::default()
        .with_title(title)
        .with_surface_size(dioxus_native::LogicalSize::new(size.0, size.1));
    #[cfg(target_os = "macos")]
    macos::launch(window, root);
    #[cfg(not(target_os = "macos"))]
    {
        let config = dioxus_native::Config::new()
            .with_window_attributes(window)
            .with_font_ctx(typefaces::notebook_fonts());
        dioxus_native::launch_cfg(root, vec![], vec![Box::new(config)]);
    }
}

fn app() -> Element {
    let args = cli::current();
    if args.is_gallery() {
        return rsx! { ui::gallery::Gallery {} };
    }
    if args.test_window {
        return notebook_view(
            "# Input check\n\nabcd\nefgh\nijkl\nmnop\n\n| A | B |\n| --- | --- |\n| one | two |\n| three | four |",
            NotebookOptions { inspect: true, ..Default::default() },
        );
    }
    let split = args.split;
    let inspect = args.test_editor;
    if let Some(path) = args.sync_file.clone() {
        return rsx! { SyncFileEditor { path, split, inspect } };
    }
    if inspect || args.demo {
        return rsx! { SampleEditor { split, inspect } };
    }
    rsx! { personal::Desktop { split } }
}

/// The sample note's options: sample comment threads unless inspecting.
fn sample_options(split: bool, inspect: bool) -> NotebookOptions {
    NotebookOptions {
        view: if split { View::Split } else { View::Live },
        inspect,
        comments: if inspect { Comments::Empty } else { Comments::Sample },
        ..Default::default()
    }
}

#[component]
fn SampleEditor(split: bool, inspect: bool) -> Element {
    notebook_view(SAMPLE, sample_options(split, inspect))
}

#[cfg(test)]
fn notebook(initial: &'static str) -> Element {
    notebook_view(initial, NotebookOptions::default())
}

#[derive(Clone)]
struct SyncFile {
    path: std::path::PathBuf,
    state: std::rc::Rc<margin_note::sync::NoteSync>,
}

#[component]
fn SyncFileEditor(path: std::path::PathBuf, split: bool, inspect: bool) -> Element {
    use margin_note::sync::NoteSync;
    let loaded = use_hook(|| {
        let sync = match NoteSync::load_from_file(&path) {
            Ok(sync) => sync,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => NoteSync::new(Note::new(SAMPLE), 1),
            Err(e) => return Err(e.to_string()),
        };
        Ok(SyncFile { path, state: std::rc::Rc::new(sync) })
    });
    let file = match loaded {
        Ok(file) => file,
        Err(error) => return rsx! { p { role: "alert", "Couldn’t open the sync file. {error}" } },
    };
    use_context_provider(|| file);
    notebook_view(SAMPLE, sample_options(split, inspect))
}

