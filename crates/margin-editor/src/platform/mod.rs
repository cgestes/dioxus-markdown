//! Where the notebook meets its renderer. The editor's code is the same on
//! every target; it reaches the rendered document and its text layouts through
//! the names this module exports.
//!
//! - Native (`blitz.rs`): Blitz's document and Parley's layouts themselves.
//! - Browser (`web.rs`): the same calls answered from the DOM.
#[cfg(not(target_arch = "wasm32"))]
mod blitz;
#[cfg(not(target_arch = "wasm32"))]
pub use blitz::*;

#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_arch = "wasm32")]
pub use web::*;

/// What `use_field_watch` reports.
pub enum FieldEvent {
    /// An input method started composing.
    Composing,
    /// The focused field's caret, text or position changed, or the window resized.
    Changed,
    /// The window resized or the page viewport scrolled.
    ViewportChanged,
}

/// How many inputs `use_input_trace` reports at most between two frames.
const TRACE_BATCH: usize = 48;

/// A shortcut's label with the primary modifier of the keyboard in use:
/// `shortcut("Shift+M")` is "⌘⇧M" on an Apple keyboard and "Ctrl+Shift+M" elsewhere.
pub fn shortcut(keys: &str) -> String {
    if apple_keys() {
        format!("⌘{}", keys.replace("Shift+", "⇧"))
    } else {
        format!("Ctrl+{keys}")
    }
}

