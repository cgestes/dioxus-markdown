//! Markdown editing commands shared by the native editor and the browser study.
#[cfg(target_arch = "wasm32")]
mod browser;
pub mod checkbox;
pub mod edit;
pub mod format;
#[cfg(not(target_arch = "wasm32"))]
pub mod note;
pub mod preview;
pub mod table;
