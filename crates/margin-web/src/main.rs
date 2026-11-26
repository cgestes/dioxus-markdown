//! `margin-web`: the notebook in a browser page. The page's address carries
//! the launch options (`?gallery`, `?test-editor`, `?test-window`). A browser
//! has no vault yet, so the note lives in memory.
#[cfg(target_arch = "wasm32")]
mod page {
    use dioxus::prelude::*;
    use margin_editor::{Comments, INPUT_CHECK, InMemory, NotebookOptions, SAMPLE, Store, notebook_view};

    /// What the status bar says about the note: a page keeps nothing yet.
    const UNSAVED: &str = "Not saved: the note lasts until this page closes";

    /// Launch options. A page has no command line, so they come from the URL's query.
    struct Options {
        gallery: bool,
        test_editor: bool,
        test_window: bool,
    }

    fn options() -> Options {
        let query = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
        let has = |flag: &str| query.trim_start_matches('?').split('&').any(|part| part.split('=').next() == Some(flag));
        Options { gallery: has("gallery"), test_editor: has("test-editor"), test_window: has("test-window") }
    }

    pub fn main() {
        margin_editor::launch("Margin", (920.0, 650.0), app);
    }

    fn app() -> Element {
        let options = options();
        if options.gallery {
            return rsx! { margin_editor::Gallery {} };
        }
        if options.test_window {
            return notebook_view(INPUT_CHECK, NotebookOptions { inspect: true, ..Default::default() });
        }
        let inspect = options.test_editor;
        let store = use_hook(|| Store::new(InMemory { status: UNSAVED.to_owned() }));
        notebook_view(
            SAMPLE,
            NotebookOptions {
                inspect,
                comments: if inspect { Comments::Empty } else { Comments::Sample },
                store: Some(store),
                ..Default::default()
            },
        )
    }
}

fn main() {
    #[cfg(target_arch = "wasm32")]
    page::main();
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("margin-web runs in a browser: dx serve --web --package margin-web");
}
