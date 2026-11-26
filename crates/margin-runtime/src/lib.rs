//! What an app runs around its interface: the window it opens (`launch`),
//! blocking work and agent turns off the UI thread (`worker`), and the restart
//! into a newer build (`restart`). Nothing here knows the notebook.
use margin_ui::prelude::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(target_arch = "wasm32"))]
pub mod restart;
pub mod worker;

/// In a browser, restarting into a newer build is the reload button.
#[cfg(target_arch = "wasm32")]
pub mod restart {
    use margin_ui::prelude::*;

    pub fn worktree_name() -> Option<String> {
        None
    }

    pub fn use_newer_build() -> Signal<bool> {
        use_signal(|| false)
    }

    pub fn restart_app() {}
}

/// The app's name, as `launch` was given it: window titles start with it.
static APP_TITLE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// A window's title for `subject` (what it shows): the app's name, the
/// worktree a development build was built in, then the subject.
pub fn window_title(subject: &str) -> String {
    let app = APP_TITLE.get().map_or("Margin", String::as_str);
    match restart::worktree_name() {
        Some(worktree) => format!("{app} · {worktree} · {subject}"),
        None => format!("{app} · {subject}"),
    }
}

/// Open a window (or, in a browser, the page) titled `title` whose content
/// is `root`, set in the bundled typefaces.
pub fn launch(title: &str, size: (f64, f64), root: fn() -> Element) {
    let _ = APP_TITLE.set(title.to_owned());
    open(title, size, root);
}

#[cfg(not(target_arch = "wasm32"))]
fn open(title: &str, size: (f64, f64), root: fn() -> Element) {
    let window = dioxus_native::WindowAttributes::default().with_title(title);
    // On iPhone and iPad the window is the screen; a requested size would crop it.
    let window = if cfg!(target_os = "ios") {
        window
    } else {
        window.with_surface_size(dioxus_native::LogicalSize::new(size.0, size.1))
    };
    #[cfg(target_os = "macos")]
    macos::launch(window, root);
    #[cfg(not(target_os = "macos"))]
    {
        let config = dioxus_native::Config::new()
            .with_window_attributes(window)
            .with_font_ctx(margin_ui::typefaces::fonts());
        dioxus_native::launch_cfg(root, vec![], vec![Box::new(config)]);
    }
}

#[cfg(target_arch = "wasm32")]
fn open(_title: &str, _size: (f64, f64), root: fn() -> Element) {
    dioxus::launch(root);
}
