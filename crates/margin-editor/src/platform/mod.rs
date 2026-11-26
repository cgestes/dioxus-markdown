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
}

/// macOS window integration: the AppKit key bridge and launch.
#[cfg(target_os = "macos")]
pub mod macos;
