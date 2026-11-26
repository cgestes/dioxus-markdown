//! Shared runtime pieces: the page reader, the background worker, and the
//! restart into a newer build.
pub mod page;
#[cfg(not(target_arch = "wasm32"))]
pub mod restart;
pub mod worker;

/// In a browser, restarting into a newer build is the reload button.
#[cfg(target_arch = "wasm32")]
pub mod restart {
    use crate::prelude::*;

    pub fn worktree_name() -> Option<String> {
        None
    }

    pub fn use_newer_build() -> Signal<bool> {
        use_signal(|| false)
    }

    pub fn restart_app() {}

    /// No other build asks a page what it was built with.
    pub fn answer_build_probe(_identity: &'static str) {}
}
