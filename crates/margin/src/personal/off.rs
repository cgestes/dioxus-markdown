//! Built without the `vault` feature: there is no personal notebook, and
//! `margin` opens the Field notes sample in its place.
use dioxus_native::prelude::*;

/// What the status bar says about a note kept nowhere: here, every note.
pub(crate) const UNSAVED: &str = "Not saved: this build has no vault (build with --features vault)";

/// `margin` without the vault: the Field notes sample, in memory.
#[component]
pub(crate) fn Desktop() -> Element {
    rsx! { crate::SampleEditor { inspect: false } }
}
