//! What a browser build puts in place of the parts that need an operating
//! system: command-line arguments and restarting into a newer build. The
//! notebook itself is the same code as the native app, built without the vault.

/// Launch options. A page has no command line, so they come from the URL's
/// query: `?split`, `?demo`, `?gallery`, `?test-editor`, `?test-window`.
pub(crate) mod cli {
    use std::path::PathBuf;
    use std::sync::OnceLock;

    /// The fields of the native command line that the notebook reads; a page has no sync file.
    #[derive(Clone, Debug, Default)]
    pub(crate) struct Cli {
        pub split: bool,
        pub sync_file: Option<PathBuf>,
        pub demo: bool,
        pub test_editor: bool,
        pub test_window: bool,
        pub gallery: bool,
    }

    impl Cli {
        pub(crate) fn is_gallery(&self) -> bool {
            self.gallery
        }
    }

    static CLI: OnceLock<Cli> = OnceLock::new();

    pub(crate) fn init() -> &'static Cli {
        CLI.get_or_init(|| {
            let query = web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default();
            let has = |flag: &str| query.trim_start_matches('?').split('&').any(|part| part.split('=').next() == Some(flag));
            Cli {
                split: has("split"),
                demo: has("demo"),
                gallery: has("gallery"),
                test_editor: has("test-editor"),
                test_window: has("test-window"),
                ..Default::default()
            }
        })
    }

    pub(crate) use init as current;
}

/// Restarting into a newer build is the browser's reload button.
pub(crate) mod restart {
    use crate::prelude::*;

    pub fn worktree_name() -> Option<String> {
        None
    }

    pub fn use_newer_build() -> Signal<bool> {
        use_signal(|| false)
    }

    pub fn restart_app() {}

    /// No other build asks a page what it was built with.
    pub fn answer_build_probe() {}
}
